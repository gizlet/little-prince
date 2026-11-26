//! The hero: a rigged, animated character loaded from a glTF file (KayKit
//! Adventurers by Kay Lousberg, CC0), drawn with the same toon light and ink
//! outline as the rest of the world.
//!
//! raylib's own animation path skins every vertex on the CPU and inverts a
//! matrix per vertex and bone, too slow for the handheld. Here raylib only
//! loads the file: the body, hat and cape are merged into one mesh carrying
//! bone indices (in the colour attribute) and weights (in the tangent
//! attribute), the bone matrices are worked out here from two blended
//! animations, and the GPU does the skinning (`Gl::draw_skinned`).

use crate::gl::Gl;
use crate::math::{basis, Frame, V3};
use raylib_sys::{self as ffi, Matrix, Mesh, Model, ModelAnimation, Texture2D, Transform};
use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::path::PathBuf;

type M = [f32; 16];

/// raymath's MatrixMultiply: `l` applied first, then `r`.
fn mul(l: &M, r: &M) -> M {
    let mut o = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            o[4 * i + j] = (0..4).map(|k| l[4 * i + k] * r[4 * k + j]).sum();
        }
    }
    o
}

fn scale(x: f32, y: f32, z: f32) -> M {
    [x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0]
}

fn translate(x: f32, y: f32, z: f32) -> M {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0]
}

/// raymath's QuaternionToMatrix.
fn rot(x: f32, y: f32, z: f32, w: f32) -> M {
    let (a2, b2, c2) = (x * x, y * y, z * z);
    let (ac, ab, bc) = (x * z, x * y, y * z);
    let (ad, bd, cd) = (w * x, w * y, w * z);
    [
        1.0 - 2.0 * (b2 + c2),
        2.0 * (ab + cd),
        2.0 * (ac - bd),
        0.0,
        2.0 * (ab - cd),
        1.0 - 2.0 * (a2 + c2),
        2.0 * (bc + ad),
        0.0,
        2.0 * (ac + bd),
        2.0 * (bc - ad),
        1.0 - 2.0 * (a2 + b2),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

fn srt(t: &Transform) -> M {
    let (s, q, p) = (t.scale, t.rotation, t.translation);
    mul(&mul(&scale(s.x, s.y, s.z), &rot(q.x, q.y, q.z, q.w)), &translate(p.x, p.y, p.z))
}

fn srt_inv(t: &Transform) -> M {
    let (s, q, p) = (t.scale, t.rotation, t.translation);
    mul(&mul(&translate(-p.x, -p.y, -p.z), &rot(-q.x, -q.y, -q.z, q.w)), &scale(1.0 / s.x, 1.0 / s.y, 1.0 / s.z))
}

fn blend(a: &Transform, b: &Transform, k: f32) -> Transform {
    let l = |x: f32, y: f32| x + (y - x) * k;
    let (qa, mut qb) = (a.rotation, b.rotation);
    if qa.x * qb.x + qa.y * qb.y + qa.z * qb.z + qa.w * qb.w < 0.0 {
        qb = ffi::Quaternion { x: -qb.x, y: -qb.y, z: -qb.z, w: -qb.w };
    }
    let q = ffi::Quaternion { x: l(qa.x, qb.x), y: l(qa.y, qb.y), z: l(qa.z, qb.z), w: l(qa.w, qb.w) };
    let n = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt().max(1e-6);
    Transform {
        translation: ffi::Vector3 {
            x: l(a.translation.x, b.translation.x),
            y: l(a.translation.y, b.translation.y),
            z: l(a.translation.z, b.translation.z),
        },
        rotation: ffi::Quaternion { x: q.x / n, y: q.y / n, z: q.z / n, w: q.w / n },
        scale: ffi::Vector3 { x: l(a.scale.x, b.scale.x), y: l(a.scale.y, b.scale.y), z: l(a.scale.z, b.scale.z) },
    }
}

/// What the hero is doing, from the player's movement.
pub struct Motion {
    pub speed: f32,
    pub grounded: bool,
    pub flying: bool,
    pub cheer: f32,
}

#[derive(Clone, Copy)]
struct Track {
    anim: usize,
    t: f32,
    rate: f32,
    looped: bool,
}

struct Ids {
    idle: usize,
    walk: usize,
    run: usize,
    jump: usize,
    land: usize,
    cheer: usize,
}

/// The outline's vertex merging grid: this many cells from feet to head.
const INK_CELLS: f32 = 24.0;

pub struct Hero {
    pub name: String,
    model: Model,
    anims: *mut ModelAnimation,
    count: usize,
    /// Two copies, written in turn, so a frame never rewrites the one the
    /// GPU may still be drawing from the frame before.
    meshes: [Mesh; 2],
    /// The same vertices with fewer triangles, for the ink outline: on the
    /// handheld's GPU every triangle costs fragment time, however small.
    ink: [Mesh; 2],
    ink_dirty: Cell<bool>,
    buf: usize,
    tex: Texture2D,
    bind_inv: Vec<M>,
    /// The bind pose and the skinning weights, and this frame's skinned vertices.
    bind_pos: Vec<f32>,
    bind_nrm: Vec<f32>,
    bone_ids: Vec<u8>,
    wts: Vec<f32>,
    out_pos: Vec<f32>,
    out_nrm: Vec<f32>,
    bones: usize,
    ids: Ids,
    cur: Track,
    prev: Option<(Track, f32)>,
    land_t: f32,
    /// The bone matrices for this frame, as columns, for the shader.
    pub palette: Vec<[f32; 4]>,
    /// Model units from the feet to the top of the head.
    pub height: f32,
    foot: f32,
}

/// Where the app's assets are: beside the binary on the device, in the
/// crate when running from `target/`.
pub fn assets_dir() -> PathBuf {
    let mut c = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent().and_then(|p| p.parent()) {
            c.push(d.join("assets"));
        }
    }
    c.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"));
    c.push(PathBuf::from("assets"));
    c.into_iter().find(|p| p.join("hero").is_dir()).unwrap_or_else(|| PathBuf::from("assets"))
}

fn cname(raw: &[std::ffi::c_char]) -> String {
    let bytes: Vec<u8> = raw.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

impl Hero {
    pub fn load(name: &str) -> Option<Hero> {
        let path = assets_dir().join("hero").join(format!("{name}.glb"));
        if !path.is_file() {
            eprintln!("littleprince: no hero model at {}", path.display());
            return None;
        }
        let c = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        unsafe {
            let model = ffi::LoadModel(c.as_ptr());
            let mut count = 0;
            let anims = ffi::LoadModelAnimations(c.as_ptr(), &mut count);
            let bones = model.skeleton.boneCount as usize;
            if bones == 0 || anims.is_null() || count == 0 || bones > 64 {
                eprintln!("littleprince: {name}.glb has no usable skeleton or animations");
                return None;
            }
            let bone_names: Vec<String> = (0..bones).map(|i| cname(&(*model.skeleton.bones.add(i)).name)).collect();
            // Merge the body, hat and cape; leave out what hangs on the hand slots.
            let (mut pos, mut nrm, mut uv, mut ids, mut w, mut idx) = (vec![], vec![], vec![], vec![], vec![], vec![]);
            let mut tex = None;
            for m in 0..model.meshCount as usize {
                let mesh = *model.meshes.add(m);
                let n = mesh.vertexCount as usize;
                if mesh.boneIndices.is_null()
                    || mesh.boneWeights.is_null()
                    || mesh.texcoords.is_null()
                    || mesh.normals.is_null()
                {
                    continue;
                }
                let bi = std::slice::from_raw_parts(mesh.boneIndices, n * 4);
                let bw = std::slice::from_raw_parts(mesh.boneWeights, n * 4);
                if bw[0] == 1.0 && bone_names.get(bi[0] as usize).is_some_and(|b| b.starts_with("handslot")) {
                    continue;
                }
                let base = (pos.len() / 3) as u16;
                pos.extend_from_slice(std::slice::from_raw_parts(mesh.vertices, n * 3));
                nrm.extend_from_slice(std::slice::from_raw_parts(mesh.normals, n * 3));
                uv.extend_from_slice(std::slice::from_raw_parts(mesh.texcoords, n * 2));
                ids.extend_from_slice(bi);
                w.extend_from_slice(bw);
                if mesh.indices.is_null() {
                    idx.extend((0..n as u16).map(|i| base + i));
                } else {
                    let tris = std::slice::from_raw_parts(mesh.indices, mesh.triangleCount as usize * 3);
                    idx.extend(tris.iter().map(|&i| base + i));
                }
                if tex.is_none() {
                    let mat = *model.materials.add(*model.meshMaterial.add(m) as usize);
                    tex = Some((*mat.maps).texture);
                }
            }
            if pos.is_empty() {
                return None;
            }
            let ys = pos.chunks(3).map(|p| p[1]);
            let (foot, top) = ys.fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(y), b.max(y)));
            let tex = tex?;
            ffi::SetTextureFilter(tex, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32);
            let ink_idx = crate::mesh::cluster(&pos, &idx, (top - foot) / INK_CELLS);
            let meshes = [0, 1].map(|_| crate::mesh::upload_skinned(&pos, &nrm, Some(&uv), &idx));
            let ink = [0, 1].map(|_| crate::mesh::upload_skinned(&pos, &nrm, None, &ink_idx));
            let bone_ids = ids;
            let bind_inv = (0..bones).map(|i| srt_inv(&*model.skeleton.bindPose.add(i))).collect();
            let names: Vec<String> = (0..count as usize).map(|i| cname(&(*anims.add(i)).name)).collect();
            let find = |want: &[&str]| want.iter().find_map(|n| names.iter().position(|x| x == n)).unwrap_or(0);
            let ids = Ids {
                idle: find(&["Idle", "Unarmed_Idle"]),
                walk: find(&["Walking_A", "Walking_B"]),
                run: find(&["Running_A", "Running_B"]),
                jump: find(&["Jump_Idle"]),
                land: find(&["Jump_Land"]),
                cheer: find(&["Cheer"]),
            };
            let cur = Track { anim: ids.idle, t: 0.0, rate: 1.0, looped: true };
            let mut h = Hero {
                name: name.to_string(),
                model,
                anims,
                count: count as usize,
                meshes,
                ink,
                ink_dirty: Cell::new(false),
                buf: 0,
                tex,
                bind_inv,
                out_pos: pos.clone(),
                out_nrm: nrm.clone(),
                bind_pos: pos,
                bind_nrm: nrm,
                bone_ids,
                wts: w,
                bones,
                ids,
                cur,
                prev: None,
                land_t: 0.0,
                palette: vec![[0.0; 4]; bones * 4],
                height: top - foot,
                foot,
            };
            h.pose();
            Some(h)
        }
    }

    fn anim(&self, i: usize) -> &ModelAnimation {
        unsafe { &*self.anims.add(i.min(self.count - 1)) }
    }

    fn duration(&self, i: usize) -> f32 {
        self.anim(i).keyframeCount.max(1) as f32 / 60.0
    }

    /// The pose of track `tr`: one transform per bone.
    fn sample(&self, tr: &Track) -> Vec<Transform> {
        let a = self.anim(tr.anim);
        let n = a.keyframeCount.max(1) as usize;
        let f = (tr.t * 60.0).max(0.0);
        let (i0, k) = (f.floor() as usize, f.fract());
        let (i0, i1) = if tr.looped { (i0 % n, (i0 + 1) % n) } else { ((i0).min(n - 1), (i0 + 1).min(n - 1)) };
        unsafe {
            let p0 = *a.keyframePoses.add(i0);
            let p1 = *a.keyframePoses.add(i1);
            (0..self.bones).map(|b| blend(&*p0.add(b), &*p1.add(b), k)).collect()
        }
    }

    fn play(&mut self, anim: usize, rate: f32, looped: bool) {
        if anim == self.cur.anim {
            self.cur.rate = rate;
            return;
        }
        // Start a walk or run on the same foot the last one ended on.
        let t = if looped && self.cur.looped {
            self.cur.t / self.duration(self.cur.anim) * self.duration(anim)
        } else {
            0.0
        };
        self.prev = Some((self.cur, 0.0));
        self.cur = Track { anim, t, rate, looped };
    }

    pub fn land(&mut self) {
        self.land_t = 0.28;
    }

    pub fn update(&mut self, dt: f32, m: &Motion) {
        self.land_t = (self.land_t - dt).max(0.0);
        let ids = (self.ids.idle, self.ids.walk, self.ids.run, self.ids.jump, self.ids.land, self.ids.cheer);
        if m.flying || !m.grounded {
            self.play(ids.3, 1.0, true);
        } else if self.land_t > 0.0 && m.speed < 0.6 {
            self.play(ids.4, 1.3, false);
        } else if m.speed > 1.25 {
            self.play(ids.2, (m.speed / 2.2).clamp(0.8, 1.4), true);
        } else if m.speed > 0.12 {
            self.play(ids.1, (m.speed / 0.75).clamp(0.5, 1.6), true);
        } else if m.cheer > 0.5 {
            self.play(ids.5, 1.0, true);
        } else {
            self.play(ids.0, 1.0, true);
        }
        self.cur.t += dt * self.cur.rate;
        if let Some((p, k)) = &mut self.prev {
            p.t += dt * p.rate;
            *k += dt / 0.18;
            if *k >= 1.0 {
                self.prev = None;
            }
        }
        self.pose();
    }

    /// Works out the bone matrices for the current blend of tracks.
    fn pose(&mut self) {
        let mut pose = self.sample(&self.cur);
        if let Some((p, k)) = self.prev {
            let old = self.sample(&p);
            let k = k * k * (3.0 - 2.0 * k);
            pose = old.iter().zip(&pose).map(|(a, b)| blend(a, b, k)).collect();
        }
        for (b, t) in pose.iter().enumerate() {
            let m = mul(&self.bind_inv[b], &srt(t));
            for c in 0..4 {
                self.palette[b * 4 + c] = [m[c * 4], m[c * 4 + 1], m[c * 4 + 2], m[c * 4 + 3]];
            }
        }
        self.skin();
    }

    /// Skins the mesh on the CPU and uploads positions and normals. A few
    /// thousand vertices take a fraction of a millisecond on a Cortex-A35,
    /// where looking bones up in the vertex shader cost the Mali-G31 several.
    fn skin(&mut self) {
        if crate::prof::skip("skin") {
            return;
        }
        let p = &self.palette;
        for v in 0..self.bind_pos.len() / 3 {
            let (x, y, z) = (self.bind_pos[v * 3], self.bind_pos[v * 3 + 1], self.bind_pos[v * 3 + 2]);
            let (nx, ny, nz) = (self.bind_nrm[v * 3], self.bind_nrm[v * 3 + 1], self.bind_nrm[v * 3 + 2]);
            let (mut o, mut n) = ([0.0f32; 3], [0.0f32; 3]);
            for k in 0..4 {
                let w = self.wts[v * 4 + k];
                if w == 0.0 {
                    continue;
                }
                let b = self.bone_ids[v * 4 + k] as usize * 4;
                let (c0, c1, c2, c3) = (p[b], p[b + 1], p[b + 2], p[b + 3]);
                for r in 0..3 {
                    o[r] += w * (c0[r] * x + c1[r] * y + c2[r] * z + c3[r]);
                    n[r] += w * (c0[r] * nx + c1[r] * ny + c2[r] * nz);
                }
            }
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            self.out_pos[v * 3..v * 3 + 3].copy_from_slice(&o);
            self.out_nrm[v * 3..v * 3 + 3].copy_from_slice(&[n[0] / l, n[1] / l, n[2] / l]);
        }
        self.buf ^= 1;
        self.upload_pose(self.meshes[self.buf]);
        self.ink_dirty.set(true);
    }

    fn upload_pose(&self, mesh: Mesh) {
        for (slot, data) in [
            (ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_POSITION, self.out_pos.as_slice()),
            (ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_NORMAL, self.out_nrm.as_slice()),
        ] {
            unsafe {
                ffi::UpdateMeshBuffer(mesh, slot as i32, data.as_ptr().cast(), std::mem::size_of_val(data) as i32, 0)
            };
        }
    }

    /// Draws the hero standing at `f` (unit axes: right, up, forward), `h` tall.
    pub fn draw(&self, gl: &Gl, f: &Frame, h: f32) {
        let k = h / self.height.max(1e-3);
        let m: Matrix = basis(f.x * k, f.y * k, f.z * k, f.o - f.y * (self.foot * k));
        let ink = gl.hero_outline_enabled().then(|| {
            let mesh = self.ink[self.buf];
            if self.ink_dirty.replace(false) {
                self.upload_pose(mesh);
            }
            mesh
        });
        gl.draw_skinned(self.meshes[self.buf], ink, m, self.tex);
    }

    /// The top of the head, for the birds' strings.
    pub fn crown(f: &Frame, h: f32) -> V3 {
        f.o + f.y * (h * 1.02)
    }

    /// Compares the bone matrices with raylib's own for one frame of the walk
    /// (a check for the capture scene `model`); returns the largest difference.
    pub fn check_against_raylib(&mut self) -> f32 {
        let anim = self.ids.walk;
        self.prev = None;
        self.cur = Track { anim, t: 10.0 / 60.0, rate: 1.0, looped: true };
        self.pose();
        unsafe {
            ffi::UpdateModelAnimation(self.model, *self.anim(anim), 10.0);
            let mut worst = 0.0f32;
            for b in 0..self.bones {
                let r = *self.model.boneMatrices.add(b);
                let theirs = [
                    r.m0, r.m1, r.m2, r.m3, r.m4, r.m5, r.m6, r.m7, r.m8, r.m9, r.m10, r.m11, r.m12, r.m13, r.m14,
                    r.m15,
                ];
                for c in 0..4 {
                    for rr in 0..4 {
                        worst = worst.max((self.palette[b * 4 + c][rr] - theirs[c * 4 + rr]).abs());
                    }
                }
            }
            worst
        }
    }
}

/// The animation names in the file, for debugging.
#[allow(dead_code)]
pub fn names(h: &Hero) -> Vec<String> {
    (0..h.count)
        .map(|i| unsafe { CStr::from_ptr((*h.anims.add(i)).name.as_ptr()) }.to_string_lossy().into_owned())
        .collect()
}
