//! Meshes built on the CPU once and uploaded to the GPU: the planets'
//! ground, grass, props and the smooth unit shapes characters are made of.
//!
//! Every vertex has a position, a normal, a colour and two texture
//! coordinates that the lit shader reads as parameters: `u` is how far the
//! vertex sways in the wind (0 at a root, 1 at a tip), `v` a sway phase for
//! grass or a glow amount for the ground. Normals are smooth wherever an ink
//! outline is drawn, so the outline has no gaps.

use crate::gl::{Gl, Ink, Rig};
use crate::math::{any_perp, place, v3, Rng, V3};
use raylib_sys::{self as ffi, Color, Mesh};
use std::f32::consts::TAU;

#[derive(Default, Clone)]
pub struct Builder {
    pos: Vec<f32>,
    nrm: Vec<f32>,
    uv: Vec<f32>,
    col: Vec<u8>,
    idx: Vec<u16>,
    /// A coarser copy built alongside, for drawing far away: ellipsoids one
    /// subdivision lower, cones and cylinders with half the sides.
    pub lo: Option<Box<Builder>>,
}

pub type Sway<'a> = &'a dyn Fn(V3) -> f32;

pub fn no_sway(_: V3) -> f32 {
    0.0
}

impl Builder {
    pub fn push(&mut self, p: V3, n: V3, uv: (f32, f32), c: Color) {
        self.pos.extend(p.arr());
        self.nrm.extend(n.arr());
        self.uv.extend([uv.0, uv.1]);
        self.col.extend([c.r, c.g, c.b, c.a]);
    }

    /// One triangle wound to face `out`, flat-shaded unless `normals` are given.
    pub fn tri(&mut self, mut t: [V3; 3], mut uv: [(f32, f32); 3], c: Color, out: V3, normals: Option<[V3; 3]>) {
        let mut n = (t[1] - t[0]).cross(t[2] - t[0]).norm();
        let mut ns = normals;
        if n.dot(out) < 0.0 {
            t.swap(1, 2);
            uv.swap(1, 2);
            if let Some(x) = &mut ns {
                x.swap(1, 2);
            }
            n = -n;
        }
        let ns = ns.unwrap_or([n; 3]);
        for k in 0..3 {
            self.push(t[k], ns[k], uv[k], c);
        }
    }

    /// The same triangles with shared vertices merged and an index buffer, so
    /// each vertex is shaded once instead of once per triangle. Stays
    /// unindexed if it would need more than 65535 vertices.
    pub fn indexed(self) -> Builder {
        if !self.idx.is_empty() {
            return self;
        }
        let n = self.pos.len() / 3;
        let mut out = Builder::default();
        let mut seen: std::collections::HashMap<[u32; 9], u16> = std::collections::HashMap::new();
        for v in 0..n {
            let mut key = [0u32; 9];
            for k in 0..3 {
                key[k] = self.pos[v * 3 + k].to_bits();
                key[3 + k] = self.nrm[v * 3 + k].to_bits();
            }
            key[6] = self.uv[v * 2].to_bits();
            key[7] = self.uv[v * 2 + 1].to_bits();
            key[8] = u32::from_le_bytes([self.col[v * 4], self.col[v * 4 + 1], self.col[v * 4 + 2], self.col[v * 4 + 3]]);
            let id = match seen.get(&key) {
                Some(&id) => id,
                None => {
                    let count = out.pos.len() / 3;
                    if count >= u16::MAX as usize {
                        return self;
                    }
                    out.pos.extend(&self.pos[v * 3..v * 3 + 3]);
                    out.nrm.extend(&self.nrm[v * 3..v * 3 + 3]);
                    out.uv.extend(&self.uv[v * 2..v * 2 + 2]);
                    out.col.extend(&self.col[v * 4..v * 4 + 4]);
                    seen.insert(key, count as u16);
                    count as u16
                }
            };
            out.idx.push(id);
        }
        out
    }

    pub fn tris(&self) -> usize {
        if self.idx.is_empty() { self.pos.len() / 9 } else { self.idx.len() / 3 }
    }

    /// `n` copies of this shape, copy `i` tagged with `u = i` (see `gl::Rig`).
    pub fn replicate(&self, n: usize) -> Builder {
        let mut b = Builder::default();
        let v = self.pos.len() / 3;
        for i in 0..n {
            b.pos.extend(&self.pos);
            b.nrm.extend(&self.nrm);
            b.col.extend(&self.col);
            for _ in 0..v {
                b.uv.extend([i as f32, 0.0]);
            }
            b.idx.extend(self.idx.iter().map(|&x| x + (i * v) as u16));
        }
        b
    }

    pub fn upload(self) -> Mesh {
        unsafe {
            let mut m: Mesh = std::mem::zeroed();
            let n = self.pos.len() / 3;
            if n == 0 {
                return m;
            }
            m.vertexCount = n as i32;
            m.triangleCount = if self.idx.is_empty() { (n / 3) as i32 } else { (self.idx.len() / 3) as i32 };
            m.vertices = copy(&self.pos);
            m.normals = copy(&self.nrm);
            m.texcoords = copy(&self.uv);
            m.colors = copy(&self.col);
            if !self.idx.is_empty() {
                m.indices = copy(&self.idx);
            }
            ffi::UploadMesh(&mut m, false);
            m
        }
    }
}

/// Copies into raylib's allocator, which UnloadMesh frees.
unsafe fn copy<T: Copy>(src: &[T]) -> *mut T {
    let bytes = std::mem::size_of_val(src).max(1);
    let p = ffi::MemAlloc(bytes as u32) as *mut T;
    std::ptr::copy_nonoverlapping(src.as_ptr(), p, src.len());
    p
}

/// A skinned mesh for `Gl::draw_skinned`: bone indices travel in the colour
/// attribute and bone weights in the tangent attribute.
pub fn upload_skinned(pos: &[f32], nrm: &[f32], uv: &[f32], ids: &[u8], w: &[f32], idx: &[u16]) -> Mesh {
    unsafe {
        let mut m: Mesh = std::mem::zeroed();
        m.vertexCount = (pos.len() / 3) as i32;
        m.triangleCount = (idx.len() / 3) as i32;
        m.vertices = copy(pos);
        m.normals = copy(nrm);
        m.texcoords = copy(uv);
        m.colors = copy(ids);
        m.tangents = copy(w);
        m.indices = copy(idx);
        ffi::UploadMesh(&mut m, true);
        m
    }
}

/// The 20 faces of an icosahedron, subdivided `n` times onto the unit sphere.
pub fn icosphere(n: u32) -> Vec<[V3; 3]> {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let p = [
        v3(-1.0, t, 0.0),
        v3(1.0, t, 0.0),
        v3(-1.0, -t, 0.0),
        v3(1.0, -t, 0.0),
        v3(0.0, -1.0, t),
        v3(0.0, 1.0, t),
        v3(0.0, -1.0, -t),
        v3(0.0, 1.0, -t),
        v3(t, 0.0, -1.0),
        v3(t, 0.0, 1.0),
        v3(-t, 0.0, -1.0),
        v3(-t, 0.0, 1.0),
    ]
    .map(V3::norm);
    const F: [[usize; 3]; 20] = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    let mut faces: Vec<[V3; 3]> = F.iter().map(|f| [p[f[0]], p[f[1]], p[f[2]]]).collect();
    for _ in 0..n {
        let mut next = Vec::with_capacity(faces.len() * 4);
        for [a, b, c] in faces {
            let ab = (a + b).norm();
            let bc = (b + c).norm();
            let ca = (c + a).norm();
            next.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        faces = next;
    }
    faces
}

/// A planet's ground: the unit sphere pushed out to `height`, smooth-shaded,
/// coloured per vertex by `colour(direction, height)` (colour, glow).
pub fn planet(subdiv: u32, height: &dyn Fn(V3) -> f32, colour: &dyn Fn(V3, f32) -> (Color, f32)) -> Builder {
    let normal = |u: V3| {
        let t1 = any_perp(u);
        let t2 = u.cross(t1);
        let e = 0.004;
        let p = u * height(u);
        let a = (u + t1 * e).norm();
        let c = (u + t2 * e).norm();
        let n = (a * height(a) - p).cross(c * height(c) - p).norm();
        if n.dot(u) < 0.0 {
            -n
        } else {
            n
        }
    };
    // Vertices repeat across faces: work each one out once.
    let mut cache: std::collections::HashMap<[u32; 3], (V3, V3, Color, f32)> = std::collections::HashMap::new();
    let mut b = Builder::default();
    for f in icosphere(subdiv) {
        let v = f.map(|u| {
            *cache.entry([u.x.to_bits(), u.y.to_bits(), u.z.to_bits()]).or_insert_with(|| {
                let h = height(u);
                let (c, g) = colour(u, h);
                (u * h, normal(u), c, g)
            })
        });
        let mut order = [0, 1, 2];
        if (v[1].0 - v[0].0).cross(v[2].0 - v[0].0).dot(v[0].0) < 0.0 {
            order = [0, 2, 1];
        }
        for k in order {
            b.push(v[k].0, v[k].1, (0.0, v[k].3), v[k].2);
        }
    }
    b
}

/// An ellipsoid at `c` with the three semi-axes `axes` (as vectors).
pub fn ellipsoid(b: &mut Builder, c: V3, axes: [V3; 3], subdiv: u32, col: &dyn Fn(V3) -> Color, sway: Sway, smooth: bool) {
    if let Some(lo) = b.lo.as_deref_mut() {
        ellipsoid(lo, c, axes, subdiv.saturating_sub(1), col, sway, smooth);
    }
    let [ax, ay, az] = axes;
    let (sx, sy, sz) = (ax.len(), ay.len(), az.len());
    let map = |u: V3| c + ax * u.x + ay * u.y + az * u.z;
    let nrm = |u: V3| (ax * (u.x / (sx * sx)) + ay * (u.y / (sy * sy)) + az * (u.z / (sz * sz))).norm();
    for f in icosphere(subdiv) {
        let p = f.map(map);
        let centre = (p[0] + p[1] + p[2]) * (1.0 / 3.0);
        let uv = p.map(|q| (sway(q), 0.0));
        let ns = if smooth { Some(f.map(nrm)) } else { None };
        b.tri(p, uv, col(centre), centre - c, ns);
    }
}

/// A smooth ring-to-ring section along `up` from (`y0`, `r0`) to (`y1`,
/// `r1`); a zero `r1` makes a cone. `cap` closes the ends.
#[allow(clippy::too_many_arguments)]
pub fn frustum(b: &mut Builder, base: V3, up: V3, sides: usize, seg: (f32, f32, f32, f32), col: Color, sway: Sway, cap: bool) {
    if let Some(lo) = b.lo.as_deref_mut() {
        frustum(lo, base, up, (sides / 2).max(3), seg, col, sway, cap);
    }
    let up = up.norm();
    let x = any_perp(up);
    let z = up.cross(x);
    let (y0, r0, y1, r1) = seg;
    let dir = |a: f32| x * a.cos() + z * a.sin();
    let side_n = |a: f32| (dir(a) * (y1 - y0) + up * (r0 - r1)).norm();
    let uv = |p: V3| (sway(p), 0.0);
    let bot = base + up * y0;
    let top = base + up * y1;
    for i in 0..sides {
        let a0 = i as f32 / sides as f32 * TAU;
        let a1 = (i + 1) as f32 / sides as f32 * TAU;
        let p00 = bot + dir(a0) * r0;
        let p01 = bot + dir(a1) * r0;
        let p10 = top + dir(a0) * r1;
        let p11 = top + dir(a1) * r1;
        let (n0, n1) = (side_n(a0), side_n(a1));
        let out = n0 + n1;
        if r1 > 1e-4 {
            b.tri([p00, p01, p11], [uv(p00), uv(p01), uv(p11)], col, out, Some([n0, n1, n1]));
            b.tri([p00, p11, p10], [uv(p00), uv(p11), uv(p10)], col, out, Some([n0, n1, n0]));
            if cap {
                b.tri([top, p10, p11], [uv(top), uv(p10), uv(p11)], col, up, None);
            }
        } else {
            let nm = side_n((a0 + a1) / 2.0);
            b.tri([p00, p01, top], [uv(p00), uv(p01), uv(top)], col, out, Some([n0, n1, nm]));
        }
        if cap && r0 > 1e-4 {
            b.tri([bot, p01, p00], [uv(bot), uv(p01), uv(p00)], col, -up, None);
        }
    }
}

/// A box at `c` with half-extents along the three `axes`.
pub fn cuboid(b: &mut Builder, c: V3, axes: [V3; 3], col: Color) {
    if let Some(lo) = b.lo.as_deref_mut() {
        cuboid(lo, c, axes, col);
    }
    for k in 0..3 {
        for s in [-1.0f32, 1.0] {
            let n = axes[k] * s;
            let u = axes[(k + 1) % 3];
            let v = axes[(k + 2) % 3];
            let f = c + n;
            let q = [f - u - v, f + u - v, f + u + v, f - u + v];
            let nn = n.norm();
            b.tri([q[0], q[1], q[2]], [(0.0, 0.0); 3], col, nn, None);
            b.tri([q[0], q[2], q[3]], [(0.0, 0.0); 3], col, nn, None);
        }
    }
}

/// Blades of grass over the ground, avoiding directions where `keep` is
/// false. Each blade is two crossed triangles; `u` is the sway weight, `v` a
/// phase. Normals point up from the planet so blades light like the ground.
pub fn grass(rng: &mut Rng, count: usize, height: &dyn Fn(V3) -> f32, keep: &dyn Fn(V3) -> bool, size: (f32, f32), palette: &[Color]) -> Builder {
    let mut b = Builder::default();
    let mut placed = 0;
    let mut tries = 0;
    while placed < count && tries < count * 40 {
        tries += 1;
        let c = rng.dir();
        if !keep(c) {
            continue;
        }
        // Blades grow in clumps of one to seven.
        let hc = height(c);
        let (t1, t2) = (any_perp(c), c.cross(any_perp(c)));
        let tint = 1.0 + rng.range(-0.08, 0.08);
        let base = palette[(rng.next() % palette.len() as u64) as usize];
        let spread = 0.07 / hc;
        for _ in 0..(1 + rng.next() % 7) {
            placed += 1;
            let n = (c + t1 * rng.range(-spread, spread) + t2 * rng.range(-spread, spread)).norm();
            let t = any_perp(n).rotate(n, rng.range(0.0, TAU));
            let root = n * (height(n) - 0.01);
            let h = rng.range(size.0, size.1);
            let w = h * rng.range(0.07, 0.13);
            let lean = t.cross(n) * (h * rng.range(-0.35, 0.35));
            let tip = root + n * h + lean;
            let phase = rng.f();
            let c = crate::gl::scale(base, tint * rng.range(0.92, 1.08));
            let root_c = crate::gl::scale(c, 0.62);
            let tip_c = crate::gl::lighten(c, 0.3);
            for side in [t, t.rotate(n, 1.57)] {
                b.push(root - side * w, n, (0.0, phase), root_c);
                b.push(root + side * w, n, (0.0, phase), root_c);
                b.push(tip, n, (1.0, phase), tip_c);
            }
        }
    }
    b
}

/// A piece of a static mesh: the triangles nearest one of 80 directions
/// around the planet, so whole pieces can be skipped when out of view.
pub struct Chunk {
    pub dir: V3,
    /// The angle from `dir` to the piece's farthest vertex.
    pub ang: f32,
    pub mesh: Mesh,
}

impl Builder {
    /// Splits the (unindexed) triangles into chunks by direction from the
    /// planet's centre, indexing each chunk when `indexed`.
    pub fn chunked(self, indexed: bool) -> Vec<Chunk> {
        let centers: Vec<V3> = icosphere(1).iter().map(|f| (f[0] + f[1] + f[2]).norm()).collect();
        let mut parts: Vec<Builder> = vec![Builder::default(); centers.len()];
        let at = |v: usize| v3(self.pos[v * 3], self.pos[v * 3 + 1], self.pos[v * 3 + 2]);
        for t in 0..self.pos.len() / 9 {
            let c = (at(t * 3) + at(t * 3 + 1) + at(t * 3 + 2)).norm();
            let i = (0..centers.len()).max_by(|&a, &b| centers[a].dot(c).total_cmp(&centers[b].dot(c))).unwrap_or(0);
            let p = &mut parts[i];
            for v in t * 3..t * 3 + 3 {
                p.pos.extend(&self.pos[v * 3..v * 3 + 3]);
                p.nrm.extend(&self.nrm[v * 3..v * 3 + 3]);
                p.uv.extend(&self.uv[v * 2..v * 2 + 2]);
                p.col.extend(&self.col[v * 4..v * 4 + 4]);
            }
        }
        parts
            .into_iter()
            .zip(centers)
            .filter(|(b, _)| !b.pos.is_empty())
            .map(|(b, dir)| {
                let ang = b.pos.chunks(3).map(|q| v3(q[0], q[1], q[2]).norm().dot(dir).clamp(-1.0, 1.0).acos()).fold(0.0, f32::max);
                let mesh = if indexed { b.indexed().upload() } else { b.upload() };
                Chunk { dir, ang, mesh }
            })
            .collect()
    }
}

/// White unit shapes, coloured and placed at draw time.
pub struct Kit {
    /// A smooth unit sphere.
    pub ball: Mesh,
    /// A cone along +Y, radius 1 at y = 0 to a point at y = 1.
    pub cone: Mesh,
    /// A truncated cone along +Y, radius 1 at y = 0 to 0.5 at y = 1.
    pub stub: Mesh,
    /// A cylinder along +Y from 0 to 1, radius 1.
    pub tube: Mesh,
    /// A box from -1 to 1.
    pub cube: Mesh,
    /// A cylinder along +Y from radius 1 at y = 0 to 0.8 at y = 1.
    pub taper: Mesh,
    /// The same shapes batched, for `Gl::set_rig`.
    pub rigs: Vec<Rig>,
}

impl Kit {
    pub fn new() -> Kit {
        let white = crate::gl::rgb(0xFFFFFF);
        let up = V3::Y;
        let ax = [v3(1.0, 0.0, 0.0), up, v3(0.0, 0.0, 1.0)];
        let shape = |f: &dyn Fn(&mut Builder)| {
            let mut b = Builder::default();
            f(&mut b);
            b
        };
        let shapes = [
            shape(&|b| ellipsoid(b, V3::ZERO, ax, 2, &|_| white, &no_sway, true)),
            shape(&|b| frustum(b, V3::ZERO, up, 10, (0.0, 1.0, 1.0, 0.0), white, &no_sway, true)),
            shape(&|b| frustum(b, V3::ZERO, up, 12, (0.0, 1.0, 1.0, 0.5), white, &no_sway, true)),
            shape(&|b| frustum(b, V3::ZERO, up, 8, (0.0, 1.0, 1.0, 1.0), white, &no_sway, true)),
            shape(&|b| cuboid(b, V3::ZERO, ax, white)),
            // Small parts (eyes, buttons, hair, leaves) use the coarser ball.
            shape(&|b| ellipsoid(b, V3::ZERO, ax, 1, &|_| white, &no_sway, true)),
            // Limbs: a gently tapering, rounded-ended cylinder.
            shape(&|b| frustum(b, V3::ZERO, up, 10, (0.0, 1.0, 1.0, 0.8), white, &no_sway, true)),
        ];
        let mut singles = Vec::new();
        let mut rigs = Vec::new();
        for b in shapes {
            let b = b.indexed();
            let per = (b.pos.len() / 3) as i32;
            let per_tris = b.tris() as i32;
            let batch = b.replicate(crate::gl::RIG_N).upload();
            let single = b.upload();
            rigs.push(Rig { single: unsafe { *single.vboId }, mesh: batch, per, per_tris });
            singles.push(single);
        }
        Kit { ball: singles[0], cone: singles[1], stub: singles[2], tube: singles[3], cube: singles[4], taper: singles[6], rigs }
    }

    /// A tapering limb from `a` (radius `r0`) to `b` (radius `r1`), with round ends.
    pub fn taper(&self, gl: &Gl, a: V3, b: V3, r0: f32, r1: f32, c: Color) {
        let d = b - a;
        gl.part(self.taper, place(a, d, any_perp(d), v3(r0, d.len(), r0)), c, 0.0, Ink::Radial);
        gl.part(self.ball, place(a, d, any_perp(d), v3(r0, r0, r0)), c, 0.0, Ink::None);
        gl.part(self.ball, place(b, d, any_perp(d), v3(r1, r1, r1)), c, 0.0, Ink::None);
    }

    /// A rounded limb from `a` to `b`, `r` thick.
    pub fn limb(&self, gl: &Gl, a: V3, b: V3, r: f32, c: Color) {
        let d = b - a;
        let len = d.len().max(1e-4);
        gl.part(self.ball, place((a + b) * 0.5, d, any_perp(d), v3(r, len * 0.5 + r * 0.5, r)), c, 0.0, Ink::Radial);
    }
}
