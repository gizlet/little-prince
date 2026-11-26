//! Particles, ground ripples, light bolts, screen shake and flash.

use crate::gl::{rgba, Sprites};
use crate::math::{any_perp, Rng, V3};
use raylib_sys::{Color, Texture2D};
use std::f32::consts::PI;

#[derive(Clone, Copy)]
pub struct Particle {
    pub p: V3,
    pub v: V3,
    pub life: f32,
    pub max: f32,
    pub size: f32,
    /// Size change per second.
    pub grow: f32,
    pub col: Color,
    /// Additive light rather than see-through dust.
    pub glow: bool,
    /// Pull towards the planet's centre; negative floats away.
    pub grav: f32,
    pub drag: f32,
    /// A bolt travelling from `.0` to `.1` over its life, arcing by `.2`.
    pub path: Option<(V3, V3, f32)>,
}

impl Particle {
    pub fn new(p: V3, v: V3, life: f32, size: f32, col: Color, glow: bool) -> Particle {
        Particle { p, v, life, max: life, size, grow: 0.0, col, glow, grav: 0.0, drag: 1.0, path: None }
    }
}

struct Ring {
    c: V3,
    n: V3,
    r: f32,
    life: f32,
    max: f32,
    col: Color,
}

pub struct Fx {
    parts: Vec<Particle>,
    rings: Vec<Ring>,
    pub shake: f32,
    pub flash: f32,
    pub rng: Rng,
}

pub struct Burst<'a> {
    pub n: usize,
    pub speed: (f32, f32),
    pub size: (f32, f32),
    pub life: (f32, f32),
    pub cols: &'a [Color],
    pub glow: bool,
    pub grav: f32,
    pub drag: f32,
    /// 0 spreads in every direction, 1 only along `up`.
    pub aim: f32,
}

impl Fx {
    pub fn new() -> Fx {
        Fx { parts: Vec::with_capacity(1024), rings: Vec::new(), shake: 0.0, flash: 0.0, rng: Rng::new(99) }
    }

    pub fn clear(&mut self) {
        self.parts.clear();
        self.rings.clear();
    }

    pub fn count(&self) -> usize {
        self.parts.len()
    }

    pub fn spawn(&mut self, p: Particle) {
        if self.parts.len() < 1500 {
            self.parts.push(p);
        }
    }

    pub fn burst(&mut self, at: V3, up: V3, b: Burst) {
        for _ in 0..b.n {
            let d = (self.rng.dir() * (1.0 - b.aim) + up * b.aim).norm();
            let v = d * self.rng.range(b.speed.0, b.speed.1);
            let col = b.cols[(self.rng.next() % b.cols.len() as u64) as usize];
            let life = self.rng.range(b.life.0, b.life.1);
            let size = self.rng.range(b.size.0, b.size.1);
            let mut p = Particle::new(at, v, life, size, col, b.glow);
            p.grav = b.grav;
            p.drag = b.drag;
            self.spawn(p);
        }
    }

    /// An expanding ring lying on the ground at `c`.
    pub fn ring(&mut self, c: V3, n: V3, r: f32, life: f32, col: Color) {
        self.rings.push(Ring { c, n, r, life, max: life, col });
    }

    pub fn update(&mut self, dt: f32) {
        self.shake = (self.shake - dt * 1.6).max(0.0);
        self.flash = (self.flash - dt * 2.2).max(0.0);
        let mut trail = Vec::new();
        for p in &mut self.parts {
            p.life -= dt;
            if let Some((a, b, arc)) = p.path {
                let u = 1.0 - (p.life / p.max).clamp(0.0, 1.0);
                let up = ((a + b) * 0.5).norm();
                let at = a.lerp(b, u) + up * ((u * PI).sin() * arc);
                if self.rng.f() < 0.7 {
                    let mut s = Particle::new(at, self.rng.dir() * 0.15, 0.45, 0.07, p.col, true);
                    s.grow = -0.1;
                    trail.push(s);
                }
                p.p = at;
                continue;
            }
            let pull = p.p.norm() * (p.grav * dt);
            p.v -= pull;
            p.v = p.v * (1.0 - (p.drag * dt).min(1.0));
            p.p += p.v * dt;
            p.size = (p.size + p.grow * dt).max(0.0);
        }
        self.parts.retain(|p| p.life > 0.0);
        for s in trail {
            self.spawn(s);
        }
        for r in &mut self.rings {
            r.life -= dt;
        }
        self.rings.retain(|r| r.life > 0.0);
    }

    fn fade(p: &Particle) -> f32 {
        let t = (p.life / p.max).clamp(0.0, 1.0);
        let born = ((p.max - p.life) * 10.0).min(1.0);
        born * if p.glow { t.sqrt() } else { t }
    }

    /// See-through dust; call inside `gl::alpha`.
    pub fn draw_dust(&self, s: &Sprites, tex: Texture2D) {
        Sprites::begin(tex);
        for p in self.parts.iter().filter(|p| !p.glow) {
            s.billboard(p.p, p.size, rgba(p.col, Fx::fade(p)));
        }
        Sprites::end();
    }

    /// Light: glowing particles and ripples; call inside `gl::additive`.
    pub fn draw_glow(&self, s: &Sprites, sprite: Texture2D, ring: Texture2D) {
        Sprites::begin(sprite);
        for p in self.parts.iter().filter(|p| p.glow) {
            let a = Fx::fade(p);
            s.billboard(p.p, p.size, rgba(p.col, a));
            if p.size > 0.06 {
                s.billboard(p.p, p.size * 0.35, rgba(crate::gl::lighten(p.col, 0.7), a));
            }
        }
        Sprites::end();
        Sprites::begin(ring);
        for r in &self.rings {
            let u = 1.0 - r.life / r.max;
            let e = 1.0 - (1.0 - u).powi(3);
            let rad = r.r * (0.15 + 0.85 * e);
            let a = any_perp(r.n);
            let b = r.n.cross(a);
            Sprites::quad(r.c + r.n * 0.03, a * rad, b * rad, rgba(r.col, (1.0 - u).powf(1.5)));
        }
        Sprites::end();
    }

    /// A camera offset for screen shake.
    pub fn shake_offset(&self, t: f32, right: V3, up: V3) -> V3 {
        let s = self.shake * self.shake * 0.12;
        (right * (t * 53.0).sin() + up * (t * 71.0 + 1.3).sin()) * s
    }
}
