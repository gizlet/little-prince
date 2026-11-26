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
            key[8] =
                u32::from_le_bytes([self.col[v * 4], self.col[v * 4 + 1], self.col[v * 4 + 2], self.col[v * 4 + 3]]);
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
        if self.idx.is_empty() {
            self.pos.len() / 9
        } else {
            self.idx.len() / 3
        }
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

impl Builder {
    /// Uploads a mesh that never changes and keeps only its indices on the
    /// CPU: raylib draws from the GPU buffers and needs the indices pointer
    /// only to know the mesh is indexed.
    pub fn upload_static(self) -> Mesh {
        let mut m = self.upload();
        unsafe {
            for p in [&mut m.vertices, &mut m.normals, &mut m.texcoords] {
                ffi::MemFree(*p as *mut _);
                *p = std::ptr::null_mut();
            }
            ffi::MemFree(m.colors as *mut _);
            m.colors = std::ptr::null_mut();
        }
        m
    }
}

impl Builder {
    /// Uploads these indexed vertices once, for a cell's four levels of
    /// detail. `far`, `near` and `close` draw the first `counts` triangles of
    /// the index list, so they share one mesh but for its triangle count;
    /// `mid`, if given, has its own index list over the same vertices.
    ///
    /// With vertex array objects the vertices go in one interleaved buffer;
    /// without them raylib binds the buffers named in `vboId` itself, so each
    /// attribute gets its own.
    fn upload_cell(&self, counts: [usize; 3], mid: Option<&[u16]>) -> [Mesh; 4] {
        let n = self.pos.len() / 3;
        let [far_n, near_n, close_n] = counts;
        if n == 0 || close_n == 0 {
            return unsafe { std::mem::zeroed() };
        }
        unsafe {
            let vao = ffi::rlLoadVertexArray();
            let mut ids = [0u32; 4];
            let attribs: [(u32, i32, u32, bool, i32); 4] = [
                (0, 3, ffi::RL_FLOAT, false, 0),
                (2, 3, ffi::RL_FLOAT, false, 12),
                (1, 2, ffi::RL_FLOAT, false, 24),
                (3, 4, ffi::RL_UNSIGNED_BYTE, true, 32),
            ];
            const STRIDE: usize = 36;
            if vao > 0 {
                let mut data = Vec::with_capacity(n * STRIDE);
                for v in 0..n {
                    for x in &self.pos[v * 3..v * 3 + 3] {
                        data.extend(x.to_ne_bytes());
                    }
                    for x in &self.nrm[v * 3..v * 3 + 3] {
                        data.extend(x.to_ne_bytes());
                    }
                    for x in &self.uv[v * 2..v * 2 + 2] {
                        data.extend(x.to_ne_bytes());
                    }
                    data.extend(&self.col[v * 4..v * 4 + 4]);
                }
                ids[0] = ffi::rlLoadVertexBuffer(data.as_ptr() as *const _, data.len() as i32, false);
            } else {
                ids[0] = ffi::rlLoadVertexBuffer(self.pos.as_ptr() as *const _, (self.pos.len() * 4) as i32, false);
                ids[1] = ffi::rlLoadVertexBuffer(self.uv.as_ptr() as *const _, (self.uv.len() * 4) as i32, false);
                ids[2] = ffi::rlLoadVertexBuffer(self.nrm.as_ptr() as *const _, (self.nrm.len() * 4) as i32, false);
                ids[3] = ffi::rlLoadVertexBuffer(self.col.as_ptr() as *const _, self.col.len() as i32, false);
            }
            // A vertex array object for an index list, or a bare index buffer
            // without them; and the buffer ids raylib looks the buffers up in.
            let level = |idx: &[u16], vao: u32| -> Mesh {
                let mut m: Mesh = std::mem::zeroed();
                m.vertexCount = n as i32;
                m.triangleCount = (idx.len() / 3) as i32;
                m.vaoId = vao;
                if vao > 0 {
                    ffi::rlEnableVertexArray(vao);
                    ffi::rlEnableVertexBuffer(ids[0]);
                    for (loc, size, ty, norm, offset) in attribs {
                        ffi::rlSetVertexAttribute(loc, size, ty as i32, norm, STRIDE as i32, offset);
                        ffi::rlEnableVertexAttribute(loc);
                    }
                }
                let ebo = ffi::rlLoadVertexBufferElement(idx.as_ptr() as *const _, (idx.len() * 2) as i32, false);
                if vao > 0 {
                    ffi::rlDisableVertexArray();
                }
                ffi::rlDisableVertexBuffer();
                ffi::rlDisableVertexBufferElement();
                // Room for every buffer raylib may look up, whatever its build.
                let vbo = ffi::MemAlloc((9 * std::mem::size_of::<u32>()) as u32) as *mut u32;
                std::ptr::write_bytes(vbo, 0, 9);
                for (k, id) in ids.iter().enumerate() {
                    *vbo.add(k) = *id;
                }
                *vbo.add(ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_INDICES as usize) = ebo;
                m.vboId = vbo;
                // raylib draws by index whenever `indices` is set; it does
                // not read them, the index buffer holds them.
                m.indices = copy(&[0u16]);
                m
            };
            let close = level(&self.idx[..close_n * 3], vao);
            let with = |count: usize| Mesh { triangleCount: count as i32, ..close };
            let mid = match mid {
                Some(idx) if !idx.is_empty() => level(idx, if vao > 0 { ffi::rlLoadVertexArray() } else { 0 }),
                _ => with(far_n),
            };
            [with(far_n), mid, with(near_n), close]
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

/// A dynamic mesh for CPU skinning, with UVs only for textured passes.
pub fn upload_skinned(pos: &[f32], nrm: &[f32], uv: Option<&[f32]>, idx: &[u16]) -> Mesh {
    unsafe {
        let mut m: Mesh = std::mem::zeroed();
        m.vertexCount = (pos.len() / 3) as i32;
        m.triangleCount = (idx.len() / 3) as i32;
        m.vertices = copy(pos);
        m.normals = copy(nrm);
        if let Some(uv) = uv {
            m.texcoords = copy(uv);
        }
        m.indices = copy(idx);
        ffi::UploadMesh(&mut m, true);
        m
    }
}

/// Fewer triangles for the same vertices: vertices in the same `cell`-sized
/// box are merged into the first of them, and triangles that collapse are
/// dropped. Returns the new index list; the vertex buffers stay as they are.
pub fn cluster(pos: &[f32], idx: &[u16], cell: f32) -> Vec<u16> {
    let mut first: std::collections::HashMap<[i32; 3], u16> = std::collections::HashMap::new();
    let rep: Vec<u16> = (0..pos.len() / 3)
        .map(|v| {
            let q = |x: f32| (x / cell).floor() as i32;
            *first.entry([q(pos[v * 3]), q(pos[v * 3 + 1]), q(pos[v * 3 + 2])]).or_insert(v as u16)
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for t in idx.chunks(3) {
        let (a, b, c) = (rep[t[0] as usize], rep[t[1] as usize], rep[t[2] as usize]);
        if a == b || b == c || a == c {
            continue;
        }
        // The same triangle twice (in any rotation) is drawn once.
        let k = if a < b && a < c {
            [a, b, c]
        } else if b < c {
            [b, c, a]
        } else {
            [c, a, b]
        };
        if seen.insert(k) {
            out.extend_from_slice(&[a, b, c]);
        }
    }
    out
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
pub fn ellipsoid(
    b: &mut Builder,
    c: V3,
    axes: [V3; 3],
    subdiv: u32,
    col: &dyn Fn(V3) -> Color,
    sway: Sway,
    smooth: bool,
) {
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
pub fn frustum(
    b: &mut Builder,
    base: V3,
    up: V3,
    sides: usize,
    seg: (f32, f32, f32, f32),
    col: Color,
    sway: Sway,
    cap: bool,
) {
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
pub fn grass(
    rng: &mut Rng,
    count: usize,
    height: &dyn Fn(V3) -> f32,
    keep: &dyn Fn(V3) -> bool,
    size: (f32, f32),
    palette: &[Color],
) -> Builder {
    grass_from(rng, count, &mut |rng| rng.dir(), height, keep, size, palette)
}

/// Blades of grass like `grass`, only within `area` (a direction and an
/// angle round it), so a small field fills up without sampling the whole
/// planet.
pub fn grass_in(
    rng: &mut Rng,
    count: usize,
    area: (V3, f32),
    height: &dyn Fn(V3) -> f32,
    keep: &dyn Fn(V3) -> bool,
    size: (f32, f32),
    palette: &[Color],
) -> Builder {
    let (centre, ang) = area;
    let (t1, t2) = (any_perp(centre), centre.cross(any_perp(centre)));
    let lo = ang.cos();
    let mut sample = |rng: &mut Rng| {
        // Uniform over the spherical cap: uniform in height along `centre`.
        let z = rng.range(lo, 1.0);
        let a = rng.range(0.0, TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        (centre * z + (t1 * a.cos() + t2 * a.sin()) * r).norm()
    };
    grass_from(rng, count, &mut sample, height, keep, size, palette)
}

fn grass_from(
    rng: &mut Rng,
    count: usize,
    sample: &mut dyn FnMut(&mut Rng) -> V3,
    height: &dyn Fn(V3) -> f32,
    keep: &dyn Fn(V3) -> bool,
    size: (f32, f32),
    palette: &[Color],
) -> Builder {
    let mut b = Builder::default();
    let mut placed = 0;
    let mut tries = 0;
    while placed < count && tries < count * 40 {
        tries += 1;
        let c = sample(rng);
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
                // Both windings, so blades show from either side with face
                // culling on and can share a draw call with the ground.
                b.push(root - side * w, n, (0.0, phase), root_c);
                b.push(root + side * w, n, (0.0, phase), root_c);
                b.push(tip, n, (1.0, phase), tip_c);
                b.push(root + side * w, n, (0.0, phase), root_c);
                b.push(root - side * w, n, (0.0, phase), root_c);
                b.push(tip, n, (1.0, phase), tip_c);
            }
        }
    }
    b
}

/// One of 320 cells of a planet, holding everything static in it at four
/// levels of detail, each a single mesh and so a single draw call: `close`
/// has the ground, the full scenery, flowers and all the grass; `near` the
/// same with every other blade of grass; `mid` the ground and the coarser
/// scenery; `far` the ground alone. On the handheld's GPU every triangle
/// costs fragment time however small it is on screen, so distant cells
/// carry fewer.
pub struct Tile {
    pub dir: V3,
    /// The angle from `dir` to the cell's farthest vertex.
    pub ang: f32,
    /// The distance from the planet's centre to the cell's highest vertex.
    pub top: f32,
    /// The distance from the planet's centre to the cell's lowest ground.
    pub low: f32,
    /// The `Group` this cell is one of.
    pub group: usize,
    pub close: Mesh,
    pub near: Mesh,
    pub mid: Mesh,
    pub far: Mesh,
}

/// Four neighbouring cells in one mesh, drawn instead of the four when they
/// are all some way off: from high above or over hills, many cells are in
/// view and each would be a draw call of its own. `mid` has the ground and
/// the coarser scenery, `far` the ground alone.
pub struct Group {
    pub mid: Mesh,
    pub far: Mesh,
}

/// Finds the nearest of the cell directions quickly: first the nearest of
/// the 80 coarse directions, then only the cells that can be nearest to a
/// point there.
struct CellFinder {
    coarse: Vec<V3>,
    candidates: Vec<Vec<usize>>,
    cells: Vec<V3>,
    /// The coarse direction each cell lies in.
    parent: Vec<usize>,
}

impl CellFinder {
    fn new() -> CellFinder {
        let reach = |faces: &[[V3; 3]]| {
            faces
                .iter()
                .map(|f| {
                    let c = (f[0] + f[1] + f[2]).norm();
                    f.iter().map(|v| v.dot(c).clamp(-1.0, 1.0).acos()).fold(0.0, f32::max)
                })
                .fold(0.0, f32::max)
        };
        let coarse_faces = icosphere(1);
        let cell_faces = icosphere(2);
        let centre = |f: &[V3; 3]| (f[0] + f[1] + f[2]).norm();
        let coarse: Vec<V3> = coarse_faces.iter().map(centre).collect();
        let cells: Vec<V3> = cell_faces.iter().map(centre).collect();
        // A point lies within `reach` of its nearest centre at either level,
        // so its nearest cell lies within both reaches of its nearest coarse
        // direction.
        let limit = (reach(&coarse_faces) + reach(&cell_faces) + 0.01).cos();
        let candidates =
            coarse.iter().map(|c| (0..cells.len()).filter(|&i| cells[i].dot(*c) >= limit).collect()).collect();
        let mut finder = CellFinder { coarse, candidates, cells, parent: Vec::new() };
        finder.parent = finder.cells.iter().map(|&c| finder.nearest_coarse(c)).collect();
        finder
    }

    fn nearest_coarse(&self, p: V3) -> usize {
        (0..self.coarse.len()).max_by(|&a, &b| self.coarse[a].dot(p).total_cmp(&self.coarse[b].dot(p))).unwrap_or(0)
    }

    fn nearest(&self, p: V3) -> usize {
        let by = |dirs: &[V3], a: usize, b: usize| dirs[a].dot(p).total_cmp(&dirs[b].dot(p));
        let k = self.nearest_coarse(p);
        self.candidates[k].iter().copied().max_by(|&a, &b| by(&self.cells, a, b)).unwrap_or(0)
    }
}

impl Builder {
    /// Splits the (unindexed) triangles by which cell direction their centre
    /// is nearest; always one builder per cell, some empty.
    fn cells(self, finder: &CellFinder) -> Vec<Builder> {
        let mut parts: Vec<Builder> = vec![Builder::default(); finder.cells.len()];
        let at = |v: usize| v3(self.pos[v * 3], self.pos[v * 3 + 1], self.pos[v * 3 + 2]);
        for t in 0..self.pos.len() / 9 {
            let c = (at(t * 3) + at(t * 3 + 1) + at(t * 3 + 2)).norm();
            parts[finder.nearest(c)].append_vertices(&self, t * 3, 3);
        }
        parts
    }

    /// Splits the (unindexed) triangles by the group of cells their centre
    /// lies in; always one builder per group.
    fn groups(self, finder: &CellFinder) -> Vec<Builder> {
        let mut parts: Vec<Builder> = vec![Builder::default(); finder.coarse.len()];
        let at = |v: usize| v3(self.pos[v * 3], self.pos[v * 3 + 1], self.pos[v * 3 + 2]);
        for t in 0..self.pos.len() / 9 {
            let c = (at(t * 3) + at(t * 3 + 1) + at(t * 3 + 2)).norm();
            parts[finder.nearest_coarse(c)].append_vertices(&self, t * 3, 3);
        }
        parts
    }

    /// Adds a strip hanging `depth` towards the centre from every open edge
    /// of these (unindexed) triangles, seen from both sides: it fills the
    /// slivers where this coarser ground meets finer ground next to it.
    fn with_skirt(mut self, depth: f32) -> Builder {
        let key = |v: usize| [self.pos[v * 3].to_bits(), self.pos[v * 3 + 1].to_bits(), self.pos[v * 3 + 2].to_bits()];
        // Each edge by its two corners: its first two vertices and how many
        // triangles share it.
        type Corner = [u32; 3];
        let mut edges: std::collections::HashMap<(Corner, Corner), (usize, usize, u32)> = Default::default();
        for t in 0..self.pos.len() / 9 {
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (t * 3 + a, t * 3 + b);
                let (ka, kb) = (key(a), key(b));
                let k = if ka < kb { (ka, kb) } else { (kb, ka) };
                edges.entry(k).or_insert((a, b, 0)).2 += 1;
            }
        }
        let mut open: Vec<(usize, usize)> = edges.into_values().filter(|e| e.2 == 1).map(|e| (e.0, e.1)).collect();
        open.sort_unstable();
        let vertex = |b: &Builder, v: usize| {
            let p = v3(b.pos[v * 3], b.pos[v * 3 + 1], b.pos[v * 3 + 2]);
            let n = v3(b.nrm[v * 3], b.nrm[v * 3 + 1], b.nrm[v * 3 + 2]);
            let c = Color { r: b.col[v * 4], g: b.col[v * 4 + 1], b: b.col[v * 4 + 2], a: b.col[v * 4 + 3] };
            (p, n, (b.uv[v * 2], b.uv[v * 2 + 1]), c)
        };
        for (a, b) in open {
            let (pa, na, ua, ca) = vertex(&self, a);
            let (pb, nb, ub, cb) = vertex(&self, b);
            let (la, lb) = (pa - pa.norm() * depth, pb - pb.norm() * depth);
            for quad in [[(pa, na, ua, ca), (pb, nb, ub, cb), (lb, nb, ub, cb), (la, na, ua, ca)]] {
                for [i, j, k] in [[0, 1, 2], [0, 2, 3], [0, 2, 1], [0, 3, 2]] {
                    for v in [quad[i], quad[j], quad[k]] {
                        self.push(v.0, v.1, v.2, v.3);
                    }
                }
            }
        }
        self
    }

    /// Splits double-sided grass blades (12 vertices each) into every other
    /// blade and the rest, so the thinned half can be drawn on its own.
    fn split_grass(&self) -> (Builder, Builder) {
        let (mut kept, mut rest) = (Builder::default(), Builder::default());
        let n = self.pos.len() / 3;
        for first in (0..n).step_by(12) {
            kept.append_vertices(self, first, (n - first).min(6));
            if first + 6 < n {
                rest.append_vertices(self, first + 6, (n - first - 6).min(6));
            }
        }
        (kept, rest)
    }

    pub fn append(&mut self, other: &Builder) {
        self.append_vertices(other, 0, other.pos.len() / 3);
    }

    fn append_vertices(&mut self, other: &Builder, first: usize, count: usize) {
        let end = first + count;
        self.pos.extend_from_slice(&other.pos[first * 3..end * 3]);
        self.nrm.extend_from_slice(&other.nrm[first * 3..end * 3]);
        self.uv.extend_from_slice(&other.uv[first * 2..end * 2]);
        self.col.extend_from_slice(&other.col[first * 4..end * 4]);
    }
}

#[derive(Clone, Copy)]
pub struct TileLayers {
    pub props: bool,
    pub flowers: bool,
    pub grass: bool,
}

/// Builds the planet's tiles from unindexed layers in world space.
/// `coarse_ground` is the same ground built coarser, for groups of cells far
/// away.
pub fn tiles(
    [ground, props, props_lo, flowers, grass]: [Builder; 5],
    coarse_ground: Builder,
    layers: TileLayers,
) -> (Vec<Tile>, Vec<Group>) {
    let finder = CellFinder::new();
    let coarse_ground = coarse_ground.groups(&finder);
    let mut cells = [ground, props, props_lo, flowers, grass].map(|layer| layer.cells(&finder).into_iter());
    let mut out = Vec::with_capacity(finder.cells.len());
    let mut group_ground = vec![Builder::default(); finder.coarse.len()];
    let mut group_scenery = vec![Builder::default(); finder.coarse.len()];
    for (i, &dir) in finder.cells.iter().enumerate() {
        let [g, p, l, d, s] = cells.each_mut().map(|layer| layer.next().unwrap());
        if g.pos.is_empty() && p.pos.is_empty() && d.pos.is_empty() && s.pos.is_empty() {
            continue;
        }
        let group = finder.parent[i];
        group_ground[group].append(&g);
        if layers.props {
            group_scenery[group].append(&l);
        }
        let ang = [&g, &p, &d, &s]
            .iter()
            .flat_map(|b| b.pos.chunks(3))
            .map(|q| v3(q[0], q[1], q[2]).norm().dot(dir).clamp(-1.0, 1.0).acos())
            .fold(0.0, f32::max);
        let top = [&g, &p, &d, &s]
            .iter()
            .flat_map(|b| b.pos.chunks(3))
            .map(|q| v3(q[0], q[1], q[2]).len())
            .fold(0.0, f32::max);
        let low = g.pos.chunks(3).map(|q| v3(q[0], q[1], q[2]).len()).fold(top, f32::min);
        let (grass, grass_rest) = if layers.grass { s.split_grass() } else { Default::default() };
        // One set of vertices per cell, in the order the levels add them, so
        // each level is a range of its triangles: far is the ground, mid
        // adds the coarse scenery, near the full scenery, flowers and half
        // the grass, and close the rest of the grass.
        let mut layers_in_order = vec![g];
        let mut ranges = [0usize; 7];
        if layers.props {
            layers_in_order.push(p);
        }
        if layers.flowers {
            layers_in_order.push(d);
        }
        layers_in_order.push(grass);
        layers_in_order.push(grass_rest);
        if layers.props {
            layers_in_order.push(l);
        }
        let mut all = Builder::default();
        let mut sizes = Vec::new();
        for b in &layers_in_order {
            sizes.push(b.pos.len() / 9);
            all.append(b);
        }
        // Triangle ranges: ground, near-only scenery (props and flowers),
        // half the grass, the other half, the coarse scenery.
        let n_props = if layers.props { 1 } else { 0 } + if layers.flowers { 1 } else { 0 };
        let ground = sizes[0];
        let scenery: usize = sizes[1..1 + n_props].iter().sum();
        let (half, rest) = (sizes[1 + n_props], sizes[2 + n_props]);
        let coarse = if layers.props { sizes[3 + n_props] } else { 0 };
        ranges[0] = ground;
        ranges[1] = ranges[0] + scenery;
        ranges[2] = ranges[1] + half;
        ranges[3] = ranges[2] + rest;
        ranges[4] = ranges[3] + coarse;
        let all = all.indexed();
        if all.idx.is_empty() {
            // Over 65,535 vertices: too many for one set of 16-bit indices.
            out.push(tile_from_levels(dir, (ang, top, low), group, layers_in_order, layers));
            continue;
        }
        let mid = (coarse > 0).then(|| [&all.idx[..ranges[0] * 3], &all.idx[ranges[3] * 3..ranges[4] * 3]].concat());
        let meshes = all.upload_cell([ranges[0], ranges[2], ranges[3]], mid.as_deref());
        out.push(Tile { dir, ang, top, low, group, far: meshes[0], mid: meshes[1], near: meshes[2], close: meshes[3] });
    }
    let upload =
        |b: Builder| if b.pos.is_empty() { unsafe { std::mem::zeroed() } } else { b.indexed().upload_static() };
    let groups = group_ground
        .into_iter()
        .zip(group_scenery)
        .zip(coarse_ground)
        .map(|((g, l), c)| {
            let mut mid = g;
            mid.append(&l);
            Group { mid: upload(mid), far: upload(c.with_skirt(0.6)) }
        })
        .collect();
    (out, groups)
}

/// A cell built as separate meshes, one per level, for when its vertices
/// do not fit one set of 16-bit indices. `layers` are in the order `tiles`
/// collects them.
fn tile_from_levels(
    dir: V3,
    (ang, top, low): (f32, f32, f32),
    group: usize,
    mut layers: Vec<Builder>,
    on: TileLayers,
) -> Tile {
    let coarse = if on.props { layers.pop() } else { None };
    let rest = layers.pop().unwrap_or_default();
    let half = layers.pop().unwrap_or_default();
    let mut near = Builder::default();
    for b in &layers {
        near.append(b);
    }
    let mut mid = layers[0].clone();
    if let Some(c) = &coarse {
        mid.append(c);
    }
    let far = layers.swap_remove(0);
    near.append(&half);
    let mut close = near.clone();
    close.append(&rest);
    let up = |b: Builder| b.indexed().upload_static();
    Tile { dir, ang, top, low, group, far: up(far), mid: up(mid), near: up(near), close: up(close) }
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
        const BALL: usize = 0;
        const COARSE_BALL: usize = 5;
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
            rigs.push(Rig { single: unsafe { *single.vboId }, mesh: batch, per, per_tris, coarse: None });
            singles.push(single);
        }
        rigs[BALL].coarse = Some((COARSE_BALL, 0.05));
        Kit {
            ball: singles[BALL],
            cone: singles[1],
            stub: singles[2],
            tube: singles[3],
            cube: singles[4],
            taper: singles[6],
            rigs,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cell_finder_agrees_with_a_search_of_every_cell() {
        let finder = CellFinder::new();
        assert_eq!(finder.cells.len(), 320);
        let mut rng = Rng::new(7);
        for _ in 0..5000 {
            let p = rng.dir();
            let best = (0..finder.cells.len())
                .max_by(|&a, &b| finder.cells[a].dot(p).total_cmp(&finder.cells[b].dot(p)))
                .unwrap();
            assert_eq!(finder.cells[finder.nearest(p)].dot(p), finder.cells[best].dot(p));
        }
    }

    #[test]
    fn every_group_has_four_cells() {
        let finder = CellFinder::new();
        for g in 0..finder.coarse.len() {
            assert_eq!(finder.parent.iter().filter(|&&p| p == g).count(), 4, "group {g}");
        }
    }

    #[test]
    fn a_small_field_fills_up_with_all_its_blades() {
        let mut rng = Rng::new(9);
        let centre = V3::Y;
        let ang = 1.6 / 25.0;
        let b = grass_in(&mut rng, 3500, (centre, ang), &|_| 25.0, &|_| true, (0.1, 0.2), &[crate::gl::rgb(0xC99E48)]);
        // Twelve vertices a blade.
        assert!(b.pos.len() / 36 >= 3500, "{} blades", b.pos.len() / 36);
        let roots_in_field = b.pos.chunks(3).all(|q| v3(q[0], q[1], q[2]).norm().dot(centre) > (ang + 0.02).cos());
        assert!(roots_in_field);
    }

    #[test]
    fn a_skirt_hangs_from_open_edges_only() {
        let mut b = Builder::default();
        let w = crate::gl::rgb(0xFFFFFF);
        let (p0, p1, p2, p3) = (v3(0.0, 10.0, 0.0), v3(1.0, 10.0, 0.0), v3(0.0, 10.0, 1.0), v3(1.0, 10.0, 1.0));
        b.tri([p0, p1, p2], [(0.0, 0.0); 3], w, V3::Y, None);
        b.tri([p1, p3, p2], [(0.0, 0.0); 3], w, V3::Y, None);
        let skirted = b.with_skirt(0.5);
        // Two triangles share one edge; the other four edges each get a quad
        // seen from both sides: four triangles of three vertices.
        assert_eq!(skirted.pos.len() / 3, 6 + 4 * 4 * 3);
    }

    #[test]
    fn split_grass_keeps_every_blade_once() {
        let mut rng = Rng::new(3);
        let g = grass(&mut rng, 101, &|_| 10.0, &|_| true, (0.1, 0.2), &[crate::gl::rgb(0x55AA55)]);
        let (kept, rest) = g.split_grass();
        assert_eq!(kept.pos.len() + rest.pos.len(), g.pos.len());
        assert!(kept.pos.len() >= rest.pos.len());
    }
}
