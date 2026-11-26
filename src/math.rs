//! Small vector and matrix helpers. raymath's functions are `static inline`
//! in raylib.h, so runtime 1's libraylib.so does not export them.

use raylib_sys::{Matrix, Vector3};
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub const ZERO: V3 = v3(0.0, 0.0, 0.0);
    pub const Y: V3 = v3(0.0, 1.0, 0.0);

    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: V3) -> V3 {
        v3(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 1e-6 {
            self * (1.0 / l)
        } else {
            V3::Y
        }
    }
    pub fn lerp(self, o: V3, t: f32) -> V3 {
        self + (o - self) * t
    }
    /// The part of `self` perpendicular to the unit vector `n`.
    pub fn flatten(self, n: V3) -> V3 {
        self - n * self.dot(n)
    }
    /// Rotates around the unit axis `k` by `a` radians (Rodrigues).
    pub fn rotate(self, k: V3, a: f32) -> V3 {
        let (s, c) = a.sin_cos();
        self * c + k.cross(self) * s + k * (k.dot(self) * (1.0 - c))
    }
    pub fn rl(self) -> Vector3 {
        Vector3 { x: self.x, y: self.y, z: self.z }
    }
    pub fn arr(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, s: f32) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}
impl AddAssign for V3 {
    fn add_assign(&mut self, o: V3) {
        *self = *self + o;
    }
}
impl SubAssign for V3 {
    fn sub_assign(&mut self, o: V3) {
        *self = *self - o;
    }
}

/// A unit vector perpendicular to the unit vector `n`.
pub fn any_perp(n: V3) -> V3 {
    let a = if n.y.abs() < 0.9 { V3::Y } else { v3(1.0, 0.0, 0.0) };
    a.cross(n).norm()
}

/// An affine transform from basis vectors (columns) and a translation.
pub fn basis(x: V3, y: V3, z: V3, t: V3) -> Matrix {
    Matrix {
        m0: x.x,
        m1: x.y,
        m2: x.z,
        m3: 0.0,
        m4: y.x,
        m5: y.y,
        m6: y.z,
        m7: 0.0,
        m8: z.x,
        m9: z.y,
        m10: z.z,
        m11: 0.0,
        m12: t.x,
        m13: t.y,
        m14: t.z,
        m15: 1.0,
    }
}

pub fn identity() -> Matrix {
    basis(v3(1.0, 0.0, 0.0), V3::Y, v3(0.0, 0.0, 1.0), V3::ZERO)
}

/// Places a model whose +Y is "up" at `pos`, standing on `up`, facing `fwd`,
/// scaled by `s` (x, y, z in model space).
pub fn place(pos: V3, up: V3, fwd: V3, s: V3) -> Matrix {
    let up = up.norm();
    let f = fwd.flatten(up);
    let f = if f.len() < 1e-4 { any_perp(up) } else { f.norm() };
    let r = up.cross(f);
    basis(r * s.x, up * s.y, f * s.z, pos)
}

/// A local coordinate frame: right `x`, up `y`, forward `z`, at `o`.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub o: V3,
    pub x: V3,
    pub y: V3,
    pub z: V3,
}

impl Frame {
    /// Standing at `o` on `up`, facing `fwd` (flattened onto the ground).
    pub fn new(o: V3, up: V3, fwd: V3) -> Frame {
        let y = up.norm();
        let z = fwd.flatten(y);
        let z = if z.len() < 1e-4 { any_perp(y) } else { z.norm() };
        Frame { o, x: y.cross(z), y, z }
    }
    pub fn dir(&self, l: V3) -> V3 {
        self.x * l.x + self.y * l.y + self.z * l.z
    }
    pub fn at(&self, l: V3) -> V3 {
        self.o + self.dir(l)
    }
    /// A unit shape at local `l`, scaled by `s` along the frame's axes.
    pub fn part(&self, l: V3, s: V3) -> Matrix {
        basis(self.x * s.x, self.y * s.y, self.z * s.z, self.at(l))
    }
    /// The frame turned by `a` radians around its own axis `k` (0 x, 1 y, 2 z).
    pub fn turned(&self, k: usize, a: f32) -> Frame {
        let axis = [self.x, self.y, self.z][k].norm();
        let r = |v: V3| v.rotate(axis, a);
        Frame { o: self.o, x: r(self.x), y: r(self.y), z: r(self.z) }
    }
    pub fn moved(&self, l: V3) -> Frame {
        Frame { o: self.at(l), ..*self }
    }
    pub fn scaled(&self, s: V3) -> Frame {
        Frame { o: self.o, x: self.x * s.x, y: self.y * s.y, z: self.z * s.z }
    }
}

pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

pub fn smooth(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

/// Frame-rate independent exponential approach: `rate` per second.
pub fn approach(cur: f32, target: f32, rate: f32, dt: f32) -> f32 {
    cur + (target - cur) * (1.0 - (-rate * dt).exp())
}

pub fn approach3(cur: V3, target: V3, rate: f32, dt: f32) -> V3 {
    cur.lerp(target, 1.0 - (-rate * dt).exp())
}

/// A deterministic random source (xorshift), so every run builds the same worlds.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// 0..1
    pub fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    /// A uniformly distributed unit vector.
    pub fn dir(&mut self) -> V3 {
        let z = self.range(-1.0, 1.0);
        let a = self.range(0.0, std::f32::consts::TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        v3(r * a.cos(), z, r * a.sin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_keeps_length_and_axis() {
        let k = v3(0.0, 0.0, 1.0);
        let r = v3(1.0, 0.0, 0.0).rotate(k, std::f32::consts::FRAC_PI_2);
        assert!((r - V3::Y).len() < 1e-5, "{r:?}");
        assert!((k.rotate(k, 1.3) - k).len() < 1e-6);
    }

    #[test]
    fn random_directions_are_unit() {
        let mut r = Rng::new(7);
        for _ in 0..100 {
            assert!((r.dir().len() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn place_builds_an_orthonormal_frame() {
        let m = place(V3::ZERO, v3(0.0, 0.0, 1.0), v3(1.0, 0.0, 0.3), v3(1.0, 1.0, 1.0));
        let x = v3(m.m0, m.m1, m.m2);
        let y = v3(m.m4, m.m5, m.m6);
        let z = v3(m.m8, m.m9, m.m10);
        assert!(x.dot(y).abs() < 1e-5 && y.dot(z).abs() < 1e-5 && x.dot(z).abs() < 1e-5);
        assert!((y - v3(0.0, 0.0, 1.0)).len() < 1e-5);
    }
}
