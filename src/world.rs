//! The three planets, after the book:
//!
//! - B612, the prince's own planet: two active volcanoes and an extinct one,
//!   the rose under her glass globe, a chair for watching sunsets, and baobab
//!   sprouts that must be pulled up before they grow.
//! - The lamplighter's planet, so small that a day lasts seconds: he lights
//!   the lamp at every dusk and puts it out at every dawn.
//! - The desert on Earth: dunes, a field of wheat, a crashed biplane and a
//!   wary fox, who comes a little closer each time the prince stands still.
//!
//! Each planet's sun circles it, and the sky is worked out from where the
//! prince stands: walking a few steps on a planet this small turns day into
//! sunset and night.

use crate::fx::{Burst, Fx, Particle};
use crate::gl::{self, lighten, mix, rgb, rgba, Gl, Ink, Sprites};
use crate::math::{any_perp, approach, place, smooth, v3, Frame, Rng, V3};
use crate::mesh::{self, no_sway, Builder, Kit, Tile};
use crate::sound::{Sfx, Sounds};
use crate::text::T;
use raylib_sys::Color;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    B612,
    Lamp,
    Desert,
}

pub const KINDS: [Kind; 3] = [Kind::B612, Kind::Lamp, Kind::Desert];
const FOX_TRUST: usize = 5;
/// Up to `CLOSE` from the camera a cell has all its grass, flowers and full
/// scenery; up to `NEAR` half the grass; up to `MID` only coarser scenery;
/// beyond, only the ground.
const CLOSE: f32 = 3.0;
const NEAR: f32 = 6.0;
const MID: f32 = 16.0;
/// How far the stars, sun and moon are drawn from the camera.
pub const SKY_D: f32 = 26.0;

/// The sky and light where the prince stands.
#[derive(Clone, Copy)]
pub struct Sky {
    pub top: Color,
    pub bottom: Color,
    pub sun: V3,
    pub sun_col: Color,
    pub shade: Color,
    pub amb: Color,
    pub rim: Color,
    pub fog: Color,
    pub stars: f32,
    pub night: f32,
    pub ink: Color,
}

struct Pal {
    day: (Color, Color),
    set: (Color, Color),
    night: (Color, Color),
    stars: f32,
    wind: V3,
    accent: Color,
}

/// What the world needs to know about the prince.
pub struct PState {
    pub feet: V3,
    pub face: V3,
    pub speed: f32,
}

fn wsum(c: [(Color, f32); 3]) -> Color {
    let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
    for (col, w) in c {
        r += col.r as f32 * w;
        g += col.g as f32 * w;
        b += col.b as f32 * w;
    }
    let q = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    Color { r: q(r), g: q(g), b: q(b), a: 255 }
}

fn bumps(n: V3, s: f32) -> f32 {
    let a = (n.x * 3.1 + s).sin() * (n.y * 2.7 + s * 1.3).sin();
    let b = (n.y * 5.3 - s).sin() * (n.z * 4.9 + s * 0.7).sin();
    let c = (n.z * 2.3 + s * 2.1).sin() * (n.x * 3.7 - s).sin();
    let d = (n.x * 9.1 + n.z * 7.3 + s).sin() * 0.25;
    a * 0.5 + b * 0.3 + c * 0.5 + d
}

fn dune(n: V3) -> f32 {
    let a = n.dot(v3(0.8, 0.2, 0.55)) * 6.0 + 0.9 * (n.dot(v3(-0.3, 0.9, 0.3)) * 4.0).sin();
    let ridge = 1.0 - a.sin().abs();
    0.24 * ridge * ridge - 0.1 + 0.05 * bumps(n, 5.0)
}

fn ground_r(kind: Kind, n: V3) -> f32 {
    match kind {
        Kind::B612 => 9.0 + 0.55 * bumps(n, 1.7) + 0.2 * bumps(n * 3.0, 4.1) + 0.05 * bumps(n * 9.0, 2.2),
        Kind::Lamp => 1.45 + 0.012 * bumps(n, 3.0),
        Kind::Desert => 4.5 + dune(n),
    }
}

fn ground_colour(kind: Kind, n: V3, h: f32) -> (Color, f32) {
    match kind {
        Kind::B612 => {
            let k = ((h - 9.0) / 0.6 * 0.5 + 0.5).clamp(0.0, 1.0);
            let mut c = mix(rgb(0x557F35), rgb(0x86AE4E), k);
            c = mix(c, rgb(0xA5B457), smooth((bumps(n * 2.0, 6.0) - 0.3) / 0.4) * 0.5);
            c = mix(c, rgb(0x46702E), smooth((bumps(n * 5.0, 3.0) - 0.4) / 0.3) * 0.4);
            let d = path_dist(n);
            c = mix(rgb(0xB89D74), c, smooth((d - 0.16) / 0.14));
            (c, 0.0)
        }
        Kind::Lamp => {
            let k = bumps(n, 9.0) * 0.5 + 0.5;
            let c = mix(rgb(0xB7A49A), rgb(0xD8C7B4), k);
            (mix(c, rgb(0xA8B488), smooth((bumps(n, 2.0) - 0.5) / 0.3) * 0.6), 0.0)
        }
        Kind::Desert => {
            let k = ((h - 4.4) / 0.28).clamp(0.0, 1.0);
            let c = mix(rgb(0xD5A472), rgb(0xF3D8A4), k);
            let ripple = (n.dot(v3(0.3, 0.2, 0.93)) * 40.0 + bumps(n, 2.0) * 3.0).sin();
            (gl::scale(c, 1.0 + ripple * 0.03), 0.0)
        }
    }
}

/// A direction `f` radians ahead of the spawn (towards +Z) and `s` to the side.
fn spot(f: f32, s: f32) -> V3 {
    V3::Y.rotate(v3(1.0, 0.0, 0.0), f).rotate(v3(0.0, 0.0, 1.0), -s).norm()
}

/// Picks `count` directions at least `sep` radians from each other and from `taken`.
fn scatter(rng: &mut Rng, count: usize, sep: f32, taken: &mut Vec<V3>, accept: &dyn Fn(V3) -> bool) -> Vec<V3> {
    let mut out = Vec::new();
    let cos = sep.cos();
    for _ in 0..count * 400 {
        if out.len() == count {
            break;
        }
        let d = rng.dir();
        if accept(d) && taken.iter().all(|t| t.dot(d) < cos) {
            out.push(d);
            taken.push(d);
        }
    }
    out
}

fn jitter(c: Color, rng: &mut Rng, k: f32) -> Color {
    gl::scale(c, 1.0 + rng.range(-k, k))
}

/// The winding dirt path on B612: from behind the spawn past the rose and the
/// volcanoes, out to the baobab and back round.
fn b612_path() -> &'static [V3] {
    static P: std::sync::OnceLock<Vec<V3>> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        let way = [
            spot(-0.28, -0.12),
            spot(-0.1, -0.02),
            spot(0.06, 0.0),
            spot(0.18, -0.08),
            spot(0.27, -0.06),
            spot(0.45, -0.12),
            spot(0.6, 0.08),
            spot(0.68, 0.3),
            spot(0.6, 0.55),
            spot(0.4, 0.58),
            spot(0.22, 0.42),
            spot(0.08, 0.26),
            spot(-0.04, 0.12),
            spot(-0.1, -0.02),
        ];
        let mut out = Vec::new();
        for w in way.windows(2) {
            let (a, b) = (w[0], w[1]);
            let side = a.cross(b).norm();
            for i in 0..24 {
                let t = i as f32 / 24.0;
                let wig = (t * TAU + a.x * 13.0).sin() * 0.012;
                out.push((a.lerp(b, t).norm() + side * wig).norm());
            }
        }
        out
    })
}

/// Distance from `n` to the path, in world units on B612.
fn path_dist(n: V3) -> f32 {
    b612_path().iter().map(|p| (n - *p).len()).fold(f32::MAX, f32::min) * 9.0
}

fn baobab(b: &mut Builder, n: V3, r: f32, rng: &mut Rng) {
    let base = n * (r - 0.1);
    let bark = rgb(0x9A7A62);
    let x = any_perp(n);
    let z = n.cross(x);
    // A bottle-shaped trunk.
    mesh::frustum(b, base, n, 14, (0.0, 0.55, 0.9, 0.68), bark, &no_sway, false);
    mesh::frustum(b, base + n * 0.9, n, 14, (0.0, 0.68, 1.5, 0.34), gl::scale(bark, 1.05), &no_sway, true);
    let top = base + n * 2.35;
    for k in 0..7 {
        let a = k as f32 / 7.0 * TAU + rng.f() * 0.4;
        let out = x * a.cos() + z * a.sin();
        let dir = (out * 0.9 + n * 0.8).norm();
        let len = rng.range(0.9, 1.4);
        mesh::frustum(b, top - n * 0.1, dir, 7, (0.0, 0.16, len, 0.06), bark, &no_sway, true);
        let end = top + dir * len;
        let s = rng.range(0.45, 0.7);
        let leaf = jitter(rgb(0x5C8F3E), rng, 0.1);
        mesh::ellipsoid(b, end + n * 0.1, [x * s, n * (s * 0.6), z * s], 1, &|_| leaf, &no_sway, true);
    }
    mesh::ellipsoid(b, top + n * 0.5, [x * 0.8, n * 0.45, z * 0.8], 1, &|_| rgb(0x548638), &no_sway, true);
}

/// A tree: a round broadleaf (three crown blobs) or a pine (stacked cones).
fn tree(b: &mut Builder, _kind: Kind, n: V3, r: f32, rng: &mut Rng) {
    let base = n * (r - 0.05);
    let h = rng.range(1.1, 1.8);
    let x = any_perp(n).rotate(n, rng.range(0.0, TAU));
    let z = n.cross(x);
    let bark = rgb(0x7A5640);
    if rng.f() < 0.35 {
        mesh::frustum(b, base, n, 7, (0.0, 0.07, h * 0.3, 0.05), bark, &no_sway, false);
        for (i, (y, rad, len)) in
            [(0.18f32, 0.42f32, 0.5f32), (0.42, 0.33, 0.42), (0.64, 0.23, 0.36)].into_iter().enumerate()
        {
            let c = jitter(if i % 2 == 0 { rgb(0x3C7A4C) } else { rgb(0x4A8A56) }, rng, 0.06);
            mesh::frustum(b, base + n * (y * h), n, 10, (0.0, rad * h * 0.8, len * h, 0.0), c, &no_sway, false);
        }
    } else {
        mesh::frustum(b, base, n, 8, (0.0, 0.1, h * 0.5, 0.06), bark, &no_sway, false);
        for (o, s) in [
            (v3(0.0, 0.72, 0.0), 0.42f32),
            (v3(0.24, 0.6, 0.12), 0.32),
            (v3(-0.22, 0.62, -0.1), 0.3),
            (v3(0.04, 0.9, -0.12), 0.28),
        ] {
            let c = base + x * (o.x * h) + n * (o.y * h) + z * (o.z * h);
            let col = jitter(rgb(0x5E9A44), rng, 0.1);
            let s = s * h;
            mesh::ellipsoid(
                b,
                c,
                [x * s, n * (s * 0.85), z * s],
                1,
                &|p| mix(gl::scale(col, 0.75), lighten(col, 0.12), smooth((p - c).norm().dot(n) * 0.5 + 0.5)),
                &no_sway,
                true,
            );
        }
    }
}

fn bush(b: &mut Builder, n: V3, r: f32, rng: &mut Rng) {
    let x = any_perp(n).rotate(n, rng.range(0.0, TAU));
    let z = n.cross(x);
    let s = rng.range(0.14, 0.32);
    let col =
        jitter([rgb(0x4E8236), rgb(0x5E9440), rgb(0x6FA348), rgb(0x8AAE4E)][(rng.next() % 4) as usize], rng, 0.08);
    for k in 0..(2 + rng.next() % 2) {
        let a = k as f32 * 2.2 + rng.f();
        let o = (x * a.cos() + z * a.sin()) * (s * 0.6);
        let ss = s * rng.range(0.6, 1.0);
        let c = gl::scale(col, rng.range(0.9, 1.12));
        mesh::ellipsoid(b, n * (r + ss * 0.45) + o, [x * ss, n * (ss * 0.8), z * ss], 1, &|_| c, &no_sway, true);
    }
}

fn mossy_rock(b: &mut Builder, n: V3, r: f32, size: f32, rng: &mut Rng) {
    let x = any_perp(n).rotate(n, rng.range(0.0, TAU));
    let z = n.cross(x);
    let c = n * (r + size * 0.12);
    let stone = jitter(rgb(0x938C86), rng, 0.08);
    let moss = rgb(0x6F8F45);
    let axes = [x * size, n * (size * rng.range(0.45, 0.7)), z * (size * rng.range(0.6, 0.9))];
    mesh::ellipsoid(
        b,
        c,
        axes,
        if size < 0.2 { 0 } else { 1 },
        &|p| mix(stone, moss, smooth(((p - c).norm().dot(n) - 0.35) / 0.3)),
        &no_sway,
        false,
    );
}

fn fence_post(b: &mut Builder, n: V3, r: f32) {
    mesh::frustum(b, n * (r - 0.03), n, 5, (0.0, 0.024, 0.3, 0.02), rgb(0x8E6342), &no_sway, true);
}

fn rock(b: &mut Builder, n: V3, r: f32, col: Color, size: f32, rng: &mut Rng) {
    let s = size * rng.range(0.7, 1.3);
    let x = any_perp(n).rotate(n, rng.range(0.0, TAU));
    let z = n.cross(x);
    let c = jitter(col, rng, 0.08);
    mesh::ellipsoid(b, n * (r + s * 0.15), [x * s, n * (s * 0.55), z * (s * 0.8)], 1, &|_| c, &no_sway, true);
}

/// A tiny flower: a stem and a round head.
fn flower(b: &mut Builder, n: V3, r: f32, col: Color, rng: &mut Rng) {
    // Seven triangles: a thin stem and a flat six-petal head facing the sky.
    let h = rng.range(0.05, 0.09);
    let base = n * (r - 0.005);
    let x = any_perp(n).rotate(n, rng.range(0.0, TAU));
    let z = n.cross(x);
    let head = base + n * h;
    let stem = rgb(0x5E9A4E);
    b.tri([base - x * 0.004, base + x * 0.004, head], [(0.0, 0.0); 3], stem, z, None);
    let rad = rng.range(0.016, 0.024);
    for k in 0..6 {
        let a0 = k as f32 / 6.0 * TAU;
        let a1 = (k + 1) as f32 / 6.0 * TAU;
        let p0 = head + (x * a0.cos() + z * a0.sin()) * rad;
        let p1 = head + (x * a1.cos() + z * a1.sin()) * rad;
        let c = if k % 2 == 0 { col } else { gl::scale(col, 0.92) };
        b.tri([head + n * 0.004, p0, p1], [(0.0, 0.0); 3], c, n, Some([n; 3]));
    }
}

struct Volcano {
    top: V3,
    n: V3,
    active: bool,
    puff: f32,
}

struct Sprout {
    n: V3,
    pos: V3,
    grow: f32,
    pulled: Option<f32>,
    phase: f32,
}

struct Rose {
    n: V3,
    pos: V3,
    open: f32,
    look: V3,
    near: bool,
}

struct LampPost {
    n: V3,
    head: V3,
    lit: bool,
    on: f32,
}

struct Keeper {
    n: V3,
    pos: V3,
    face: V3,
    /// The lighting or snuffing in progress: seconds in, and the new state.
    anim: Option<(f32, bool)>,
    raise: f32,
}

struct Fox {
    n: V3,
    face: V3,
    trust: usize,
    still: f32,
    phase: f32,
    speed: f32,
    sit: f32,
    wag: f32,
    heart_t: f32,
}

impl Fox {
    fn new(spawn: V3) -> Self {
        let n = spot(0.56, 0.14);
        Self {
            n,
            face: (spawn - n).flatten(n).norm(),
            trust: 0,
            still: 0.0,
            phase: 0.0,
            speed: 0.0,
            sit: 1.0,
            wag: 0.0,
            heart_t: 0.0,
        }
    }

    fn preferred_distance(trust: usize) -> f32 {
        2.4 - trust as f32 * 0.28
    }

    /// Advances the fox and returns whether it gained a point of trust.
    fn update(&mut self, dt: f32, p: &PState, fx: &mut Fx, snd: &mut Sounds) -> bool {
        let mut scored = false;
        let r = ground_r(Kind::Desert, self.n);
        let pos = self.n * r;
        let to_p = (p.feet - pos).flatten(self.n);
        let d = (p.feet - pos).len();
        let moving = p.speed > 0.35;
        let mut target: Option<(V3, f32)> = None;
        if self.trust < FOX_TRUST {
            let keep = Self::preferred_distance(self.trust);
            if d < keep - 0.3 {
                target = Some((pos - to_p.norm(), if moving { 1.8 } else { 0.9 }));
                self.still = 0.0;
            } else if !moving && d < keep + 0.9 {
                self.still += dt;
                if self.still > 2.4 {
                    self.still = 0.0;
                    self.trust += 1;
                    scored = true;
                    fx.burst(
                        pos + self.n * 0.35,
                        self.n,
                        Burst {
                            n: 10,
                            speed: (0.2, 0.5),
                            size: (0.04, 0.07),
                            life: (1.0, 1.6),
                            cols: &[rgb(0xFF8FA6), rgb(0xFFC2D0)],
                            glow: true,
                            grav: -0.25,
                            drag: 1.2,
                            aim: 0.8,
                        },
                    );
                    snd.play(Sfx::Chime(self.trust + 1), 1.0, 0.45);
                    if self.trust == FOX_TRUST {
                        fx.ring(pos, self.n, 1.2, 1.0, rgb(0xFFC15E));
                        snd.play(Sfx::Chime(7), 1.0, 0.5);
                    }
                }
            } else {
                self.still = (self.still - dt).max(0.0);
            }
            // Close the gap as trust grows while the prince waits.
            if d > keep && (!moving || d > keep + 1.8) {
                target = Some((p.feet, ((d - keep) * 2.0).min(0.8)));
            }
        } else {
            let right = p.feet.norm().cross(p.face);
            let spot = p.feet - p.face * 0.55 + right * 0.4;
            if (spot - pos).len() > 0.25 {
                target = Some((spot, (((spot - pos).len() - 0.1) * 3.0).min(2.6)));
            }
            self.heart_t -= dt;
            if self.heart_t < 0.0 {
                self.heart_t = 2.5;
                let mut h = Particle::new(pos + self.n * 0.35, self.n * 0.25, 1.2, 0.05, rgb(0xFF8FA6), true);
                h.drag = 1.0;
                fx.spawn(h);
            }
        }
        match target {
            Some((tp, speed)) => {
                let dir = (tp - pos).flatten(self.n).norm();
                self.speed = approach(self.speed, speed, 6.0, dt);
                let ang = self.speed * dt / r;
                self.n = self.n.rotate(self.n.cross(dir).norm(), ang).norm();
                self.face = crate::math::approach3(self.face, dir, 8.0, dt).flatten(self.n).norm();
                self.sit = approach(self.sit, 0.0, 8.0, dt);
            }
            None => {
                self.speed = approach(self.speed, 0.0, 8.0, dt);
                let look = to_p.norm();
                self.face = crate::math::approach3(self.face, look, 3.0, dt).flatten(self.n).norm();
                self.sit = approach(self.sit, 1.0, 3.0, dt);
            }
        }
        self.phase += self.speed * dt * 13.0;
        let wag = if self.trust >= FOX_TRUST { 10.0 } else { 2.5 + self.trust as f32 };
        self.wag += dt * wag;
        scored
    }
}

struct Star {
    d: V3,
    size: f32,
    phase: f32,
    col: Color,
}

struct Shooting {
    p: V3,
    v: V3,
    life: f32,
}

pub struct World {
    pub kind: Kind,
    pub radius: f32,
    pal: Pal,
    sun0: V3,
    period: f32,
    pub spawn: V3,
    tiles: Vec<Tile>,
    clouds: Vec<(V3, V3, f32, f32)>,
    butterflies: Vec<(V3, f32, Color)>,
    volcanoes: Vec<Volcano>,
    sprouts: Vec<Sprout>,
    rose: Option<Rose>,
    lamp: Option<LampPost>,
    keeper: Option<Keeper>,
    fox: Option<Fox>,
    /// The biplane's propeller hub: position, forward and up.
    prop: Option<Frame>,
    stars: Vec<Star>,
    shooting: Vec<Shooting>,
    shoot_t: f32,
    last_e: f32,
    rng: Rng,
    pub score: usize,
    pub total: usize,
    pub pop: f32,
}

impl World {
    pub fn new(kind: Kind) -> World {
        let mut rng = Rng::new(11 + kind as u64 * 977);
        let (pal, sun0, period) = match kind {
            Kind::B612 => (
                Pal {
                    day: (rgb(0x4A8BD8), rgb(0xD2E8F2)),
                    set: (rgb(0x4C5C9E), rgb(0xF6A77C)),
                    night: (rgb(0x0E1434), rgb(0x2C3068)),
                    stars: 1.0,
                    wind: v3(0.7, 0.1, 0.7).norm(),
                    accent: rgb(0xFFC15E),
                },
                v3(0.3, 0.78, -0.55).norm(),
                240.0,
            ),
            Kind::Lamp => (
                Pal {
                    day: (rgb(0x8EC3EE), rgb(0xEAF2EE)),
                    set: (rgb(0x5B4E91), rgb(0xF29B7A)),
                    night: (rgb(0x0B1030), rgb(0x23295C)),
                    stars: 1.0,
                    wind: v3(0.3, 0.1, 0.95).norm(),
                    accent: rgb(0xFFD27A),
                },
                v3(0.0, 0.75, -0.66).norm(),
                14.0,
            ),
            Kind::Desert => (
                Pal {
                    day: (rgb(0x9CCBEA), rgb(0xF7E3BC)),
                    set: (rgb(0x5A4C8E), rgb(0xF4A06B)),
                    night: (rgb(0x0A0F2B), rgb(0x2B2554)),
                    stars: 1.3,
                    wind: v3(0.9, 0.05, 0.4).norm(),
                    accent: rgb(0xFF9F6B),
                },
                v3(0.25, -0.05, -0.97).norm(),
                200.0,
            ),
        };
        let spawn = V3::Y;
        let r = move |n: V3| ground_r(kind, n);
        let subdiv = match kind {
            Kind::B612 => 5,
            Kind::Desert => 5,
            Kind::Lamp => 4,
        };
        let ground = mesh::planet(subdiv, &r, &|n, h| ground_colour(kind, n, h));
        let grass;
        let mut stars: Vec<Star> = (0..360)
            .map(|_| Star { d: rng.dir(), size: rng.range(0.12, 0.42), phase: rng.range(0.0, TAU), col: rgb(0xE8EEFF) })
            .collect();
        // A few big stars, some of them tinted, the ones that seem to laugh.
        for k in 0..14 {
            let col = [rgb(0xFFF1B8), rgb(0xBFD8FF), rgb(0xFFD0E0)][k % 3];
            stars.push(Star { d: rng.dir(), size: rng.range(0.7, 1.1), phase: rng.range(0.0, TAU), col });
        }
        let mut w = World {
            kind,
            radius: r(spawn),
            pal,
            sun0,
            period,
            spawn,
            tiles: Vec::new(),
            clouds: Vec::new(),
            butterflies: Vec::new(),
            volcanoes: Vec::new(),
            sprouts: Vec::new(),
            rose: None,
            lamp: None,
            keeper: None,
            fox: None,
            prop: None,
            stars,
            shooting: Vec::new(),
            shoot_t: 1.0,
            last_e: 1.0,
            rng: Rng::new(kind as u64 + 3),
            score: 0,
            total: 0,
            pop: 0.0,
        };
        let mut props = Builder::default();
        props.lo = Some(Box::default());
        let mut decor = Builder::default();
        let mut taken = vec![spawn];
        match kind {
            Kind::B612 => {
                let r = &r;
                let off_path = |d: V3, w: f32| path_dist(d) > w;
                // Two smoking volcanoes and an extinct one, as in the book.
                for (i, n) in [spot(0.3, -0.2), spot(0.37, -0.07), spot(0.33, 0.3)].into_iter().enumerate() {
                    taken.push(n);
                    let active = i < 2;
                    let k = 2.2;
                    let base = n * (r(n) - 0.06);
                    let col = if active { rgb(0x9C7462) } else { rgb(0x8E8896) };
                    mesh::frustum(&mut props, base, n, 20, (0.0, 0.22 * k, 0.21 * k, 0.1 * k), col, &no_sway, false);
                    mesh::frustum(
                        &mut props,
                        base + n * (0.2 * k),
                        n,
                        20,
                        (0.0, 0.105 * k, 0.025 * k, 0.08 * k),
                        gl::scale(col, 0.85),
                        &no_sway,
                        true,
                    );
                    let x = any_perp(n);
                    mesh::ellipsoid(
                        &mut props,
                        base + n * (0.222 * k),
                        [x * (0.085 * k), n * 0.01, n.cross(x) * (0.085 * k)],
                        2,
                        &|_| rgb(0x4A3530),
                        &no_sway,
                        true,
                    );
                    w.volcanoes.push(Volcano { top: base + n * (0.23 * k), n, active, puff: rng.f() });
                }
                let rn = spot(0.11, 0.06);
                taken.push(rn);
                w.rose = Some(Rose { n: rn, pos: rn * r(rn), open: 0.6, look: any_perp(rn), near: false });
                // A chair for watching sunsets, turned towards the sun.
                let cn = spot(-0.08, 0.14);
                taken.push(cn);
                let f = Frame::new(cn * (r(cn) - 0.01), cn, -w.sun0);
                let wood = rgb(0xB57D52);
                for (x, z) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    mesh::frustum(
                        &mut props,
                        f.at(v3(x * 0.07, 0.0, z * 0.07)),
                        f.y,
                        6,
                        (0.0, 0.012, 0.15, 0.012),
                        wood,
                        &no_sway,
                        true,
                    );
                }
                mesh::cuboid(&mut props, f.at(v3(0.0, 0.16, 0.0)), [f.x * 0.095, f.y * 0.014, f.z * 0.095], wood);
                for x in [-1.0f32, 1.0] {
                    mesh::frustum(
                        &mut props,
                        f.at(v3(x * 0.07, 0.16, -0.08)),
                        f.y,
                        6,
                        (0.0, 0.012, 0.18, 0.012),
                        wood,
                        &no_sway,
                        true,
                    );
                }
                mesh::cuboid(&mut props, f.at(v3(0.0, 0.3, -0.08)), [f.x * 0.09, f.y * 0.04, f.z * 0.012], wood);
                // The baobab that was not pulled up in time: a landmark on the far hill.
                let bn = spot(0.72, 0.38);
                taken.push(bn);
                baobab(&mut props, bn, r(bn), &mut rng);
                // A wooden fence along the path by the rose.
                let path = b612_path();
                for (i, w2) in path[30..58].windows(2).enumerate().step_by(3) {
                    let (a, b) = (w2[0], w2[1]);
                    let side = a.cross(b - a).norm() * (0.06 / 9.0);
                    let p0 = (a + side * 6.0).norm();
                    let _ = i;
                    fence_post(&mut props, p0, r(p0));
                }
                for k in (30..55).step_by(3) {
                    let (a, b) = (path[k], path[k + 3]);
                    let side = a.cross(b - a).norm() * (0.36 / 9.0);
                    let (p0, p1) = ((a + side).norm(), (b + side).norm());
                    for h in [0.11f32, 0.21] {
                        let (q0, q1) = (p0 * (r(p0) + h), p1 * (r(p1) + h));
                        let d = q1 - q0;
                        let up = p0.lerp(p1, 0.5).norm();
                        let x = d.cross(up).norm();
                        mesh::cuboid(&mut props, (q0 + q1) * 0.5, [d * 0.5, up * 0.014, x * 0.012], rgb(0xA8754C));
                    }
                }
                for n in scatter(&mut rng, 7, 0.09, &mut taken, &|d| d.dot(V3::Y) > 0.9 && off_path(d, 0.35)) {
                    w.sprouts.push(Sprout {
                        n,
                        pos: n * r(n),
                        grow: rng.range(0.6, 1.0),
                        pulled: None,
                        phase: rng.f() * TAU,
                    });
                }
                for n in scatter(&mut rng, 30, 0.14, &mut taken, &|d| off_path(d, 0.9) && d.dot(V3::Y) < 0.985) {
                    tree(&mut props, Kind::B612, n, r(n), &mut rng);
                }
                for n in scatter(&mut rng, 70, 0.05, &mut taken, &|d| off_path(d, 0.5)) {
                    bush(&mut props, n, r(n), &mut rng);
                }
                for n in scatter(&mut rng, 140, 0.035, &mut taken, &|d| off_path(d, 0.35)) {
                    let size = if rng.f() < 0.2 { rng.range(0.25, 0.5) } else { rng.range(0.06, 0.18) };
                    mossy_rock(&mut props, n, r(n), size, &mut rng);
                }
                // Patches of flowers: daisies, pink, yellow and lilac.
                let flowers =
                    [rgb(0xFFFFFF), rgb(0xFFF6E0), rgb(0xFFB3C8), rgb(0xFFE070), rgb(0xC6A8F0), rgb(0xFF8FA3)];
                let mut placed = 0;
                while placed < 600 {
                    let c = rng.dir();
                    if !off_path(c, 0.35) || taken.iter().any(|t| t.dot(c) > 0.9996) {
                        continue;
                    }
                    placed += 1;
                    let col = flowers[(rng.next() % flowers.len() as u64) as usize];
                    let t1 = any_perp(c);
                    let t2 = c.cross(t1);
                    for _ in 0..(2 + rng.next() % 5) {
                        let n = (c + t1 * rng.range(-0.025, 0.025) + t2 * rng.range(-0.025, 0.025)).norm();
                        flower(&mut decor, n, r(n), col, &mut rng);
                    }
                }
                let bare = taken.clone();
                let keep = move |d: V3| path_dist(d) > 0.28 && bare.iter().all(|t| t.dot(d) < 0.99985);
                grass = mesh::grass(
                    &mut rng,
                    15000,
                    r,
                    &keep,
                    (0.09, 0.21),
                    &[rgb(0x5A8C3A), rgb(0x6C9C42), rgb(0x7FAD4C), rgb(0x93BB58), rgb(0x4D7A33), rgb(0xA3B95A)],
                );
                for _ in 0..18 {
                    let d = rng.dir();
                    w.clouds.push((d, any_perp(d), rng.range(0.006, 0.014), rng.range(0.9, 1.6)));
                }
                for _ in 0..40 {
                    let d = (V3::Y + rng.dir() * 0.4).norm();
                    let c = [rgb(0xFFF3A8), rgb(0xFFFFFF), rgb(0xFFB0C8), rgb(0xA8D8FF), rgb(0xFFC070)]
                        [(rng.next() % 5) as usize];
                    w.butterflies.push((d, rng.f() * TAU, c));
                }
                w.total = w.sprouts.len();
            }
            Kind::Lamp => {
                let ln = spot(0.42, 0.05);
                taken.push(ln);
                let base = ln * (r(ln) - 0.02);
                let iron = rgb(0x34304A);
                mesh::frustum(&mut props, base, ln, 12, (0.0, 0.07, 0.08, 0.045), iron, &no_sway, true);
                mesh::frustum(&mut props, base, ln, 10, (0.0, 0.02, 0.84, 0.016), iron, &no_sway, true);
                mesh::frustum(&mut props, base + ln * 0.84, ln, 10, (0.0, 0.03, 0.03, 0.03), iron, &no_sway, true);
                w.lamp = Some(LampPost { n: ln, head: base + ln * 0.93, lit: false, on: 0.0 });
                let kn = ln.rotate(any_perp(ln).rotate(ln, 1.2), 0.17);
                taken.push(kn);
                w.keeper = Some(Keeper {
                    n: kn,
                    pos: kn * r(kn),
                    face: (base - kn * r(kn)).flatten(kn).norm(),
                    anim: None,
                    raise: 0.0,
                });
                for n in scatter(&mut rng, 5, 0.4, &mut taken, &|_| true) {
                    rock(&mut props, n, r(n), rgb(0xA89A92), 0.06, &mut rng);
                }
                for _ in 0..30 {
                    let n = rng.dir();
                    if taken.iter().all(|t| t.dot(n) < 0.993) {
                        let c = [rgb(0xFFFFFF), rgb(0xFFB8C8), rgb(0xFFE27A)][(rng.next() % 3) as usize];
                        flower(&mut decor, n, r(n), c, &mut rng);
                    }
                }
                let bare = taken.clone();
                grass = mesh::grass(
                    &mut rng,
                    300,
                    &r,
                    &|d| bare.iter().all(|t| t.dot(d) < 0.993),
                    (0.03, 0.07),
                    &[rgb(0x8FB070), rgb(0xA0BE7A)],
                );
                w.total = 44;
            }
            Kind::Desert => {
                // The biplane, nose down in the sand.
                let pn = spot(0.3, -0.2);
                taken.push(pn);
                let heading = any_perp(pn).rotate(pn, 0.9);
                let up = pn.rotate(heading, 0.16).rotate(pn.cross(heading), 0.12).norm();
                let f = Frame::new(pn * (r(pn) + 0.06), up, heading);
                let red = rgb(0xC9473E);
                let cream = rgb(0xEFE6D2);
                let ax = |l: V3| [f.x * l.x, f.y * l.y, f.z * l.z];
                mesh::ellipsoid(
                    &mut props,
                    f.at(v3(0.0, 0.08, 0.0)),
                    ax(v3(0.13, 0.12, 0.55)),
                    2,
                    &|_| cream,
                    &no_sway,
                    true,
                );
                mesh::frustum(
                    &mut props,
                    f.at(v3(0.0, 0.08, 0.45)),
                    f.z,
                    16,
                    (0.0, 0.115, 0.12, 0.07),
                    red,
                    &no_sway,
                    true,
                );
                mesh::ellipsoid(
                    &mut props,
                    f.at(v3(0.0, 0.18, 0.02)),
                    ax(v3(0.07, 0.04, 0.1)),
                    1,
                    &|_| rgb(0x3B3340),
                    &no_sway,
                    true,
                );
                mesh::cuboid(&mut props, f.at(v3(0.0, 0.0, 0.12)), ax(v3(0.78, 0.018, 0.13)), red);
                mesh::cuboid(&mut props, f.at(v3(0.0, 0.3, 0.16)), ax(v3(0.82, 0.018, 0.13)), lighten(red, 0.1));
                for x in [-0.55f32, -0.25, 0.25, 0.55] {
                    for z in [0.07f32, 0.24] {
                        mesh::frustum(
                            &mut props,
                            f.at(v3(x, 0.0, z)),
                            f.y,
                            5,
                            (0.0, 0.01, 0.3, 0.01),
                            rgb(0x5A4A42),
                            &no_sway,
                            true,
                        );
                    }
                }
                mesh::cuboid(&mut props, f.at(v3(0.0, 0.1, -0.52)), ax(v3(0.26, 0.014, 0.07)), red);
                mesh::cuboid(&mut props, f.at(v3(0.0, 0.2, -0.53)), ax(v3(0.014, 0.11, 0.08)), red);
                for x in [-0.15f32, 0.15] {
                    mesh::ellipsoid(
                        &mut props,
                        f.at(v3(x, -0.1, 0.2)),
                        ax(v3(0.02, 0.06, 0.06)),
                        1,
                        &|_| rgb(0x3B3340),
                        &no_sway,
                        true,
                    );
                }
                w.prop = Some(f.moved(v3(0.0, 0.08, 0.6)));
                let fox = Fox::new(spawn);
                taken.push(fox.n);
                w.fox = Some(fox);
                let wheat = spot(0.1, 0.3);
                for n in scatter(&mut rng, 12, 0.12, &mut taken, &|d| d.dot(wheat) < 0.96) {
                    rock(&mut props, n, r(n), rgb(0xB9937A), 0.12, &mut rng);
                }
                for n in scatter(&mut rng, 10, 0.1, &mut taken, &|d| d.dot(wheat) < 0.96) {
                    let c = jitter(rgb(0x9C8F5A), &mut rng, 0.1);
                    let x = any_perp(n);
                    mesh::ellipsoid(
                        &mut decor,
                        n * (r(n) + 0.03),
                        [x * 0.07, n * 0.05, n.cross(x) * 0.07],
                        1,
                        &|_| c,
                        &no_sway,
                        true,
                    );
                }
                let bare = taken.clone();
                let field = move |d: V3| d.dot(wheat) > 0.982 && bare.iter().all(|t| t.dot(d) < 0.9993);
                grass = mesh::grass(
                    &mut rng,
                    1600,
                    &r,
                    &field,
                    (0.1, 0.22),
                    &[rgb(0xC99E48), rgb(0xB68C3C), rgb(0xD6B25A)],
                );
                w.total = FOX_TRUST;
            }
        }
        let lo = props.lo.take().map(|b| *b).unwrap_or_default();
        w.tiles = mesh::tiles(
            [ground, props, lo, decor, grass],
            mesh::TileLayers {
                props: !crate::prof::skip("props"),
                flowers: !crate::prof::skip("flowers"),
                grass: !crate::prof::skip("grass"),
            },
        );
        w
    }

    pub fn name(&self) -> (T, T, T) {
        match self.kind {
            Kind::B612 => (T::B612, T::B612Hint, T::Sprouts),
            Kind::Lamp => (T::Lamp, T::LampHint, T::Sunsets),
            Kind::Desert => (T::Desert, T::DesertHint, T::Tamed),
        }
    }

    pub fn surface(&self, n: V3) -> f32 {
        ground_r(self.kind, n)
    }

    pub fn sun(&self, t: f32) -> V3 {
        self.sun0.rotate(v3(1.0, 0.0, 0.0), t * TAU / self.period).norm()
    }

    /// The sky over `n` at time `t`: day, sunset or night by the sun's height there.
    pub fn wind(&self) -> V3 {
        self.pal.wind
    }

    pub fn accent(&self) -> Color {
        self.pal.accent
    }

    pub fn sky(&self, n: V3, t: f32) -> Sky {
        let sun = self.sun(t);
        let e = n.norm().dot(sun);
        let wd = smooth((e - 0.05) / 0.35);
        let wn = smooth((-e - 0.08) / 0.3);
        let ws = (1.0 - wd - wn).max(0.0);
        let p = &self.pal;
        let w3 = |d: Color, s: Color, n: Color| wsum([(d, wd), (s, ws), (n, wn)]);
        Sky {
            top: w3(p.day.0, p.set.0, p.night.0),
            bottom: w3(p.day.1, p.set.1, p.night.1),
            sun,
            sun_col: w3(rgb(0xFFF3DC), rgb(0xFFB680), rgb(0xA9B8FF)),
            shade: w3(rgb(0x9A92C6), rgb(0x8E6FA0), rgb(0x3E4878)),
            amb: w3(p.day.0, p.set.1, p.night.1),
            rim: w3(rgb(0xFFF1DA), rgb(0xFFC48E), rgb(0x8FA6FF)),
            fog: w3(p.day.1, p.set.1, p.night.1),
            stars: (wn + ws * 0.4) * p.stars,
            night: wn,
            ink: w3(rgb(0x4A3540), rgb(0x45304A), rgb(0x1E1A3C)),
        }
    }

    /// The lamp's light, if there is one and it is lit.
    pub fn lamp_light(&self) -> Option<(V3, f32)> {
        self.lamp.as_ref().filter(|l| l.on > 0.01).map(|l| (l.head, l.on))
    }

    /// Where autoplay should walk: a point, how near to stop, and whether
    /// to stand still once there.
    pub fn autoplay_target(&self, from: V3) -> Option<(V3, f32, bool)> {
        match self.kind {
            Kind::B612 => {
                let near = |a: &V3, b: &V3| (*a - from).len().total_cmp(&(*b - from).len());
                let ready: Vec<V3> =
                    self.sprouts.iter().filter(|s| s.pulled.is_none() && s.grow > 0.55).map(|s| s.pos).collect();
                match ready.into_iter().min_by(near) {
                    Some(p) => Some((p, 0.0, false)),
                    None => self.rose.as_ref().map(|r| (r.pos, 0.55, true)),
                }
            }
            Kind::Lamp => None,
            Kind::Desert => self.fox.as_ref().filter(|f| f.trust < FOX_TRUST).map(|f| {
                let r = ground_r(self.kind, f.n);
                (f.n * r, Fox::preferred_distance(f.trust) + 0.35, true)
            }),
        }
    }

    fn set_score(&mut self, s: usize) -> bool {
        let up = s > self.score;
        if up {
            self.pop = 1.0;
        }
        self.score = s;
        up
    }

    /// Advances the planet. Returns true when the prince achieved something.
    pub fn update(&mut self, dt: f32, t: f32, p: &PState, fx: &mut Fx, snd: &mut Sounds) -> bool {
        self.pop = (self.pop - dt * 2.5).max(0.0);
        let mut scored = false;
        let up = p.feet.norm();
        let sun = self.sun(t);
        let wind = self.pal.wind;
        match self.kind {
            Kind::B612 => {
                for v in &mut self.volcanoes {
                    if !v.active {
                        continue;
                    }
                    v.puff -= dt;
                    if v.puff < 0.0 {
                        v.puff = 0.35 + self.rng.f() * 0.3;
                        let vel = v.n * self.rng.range(0.18, 0.3) + wind * 0.08 + self.rng.dir() * 0.03;
                        let mut s =
                            Particle::new(v.top, vel * 1.8, self.rng.range(2.5, 3.5), 0.1, rgb(0xE6DEE0), false);
                        s.grow = 0.16;
                        s.drag = 0.2;
                        s.grav = -0.02;
                        fx.spawn(s);
                    }
                }
                let mut pulled = 0;
                for (i, s) in self.sprouts.iter_mut().enumerate() {
                    match &mut s.pulled {
                        Some(age) => {
                            *age += dt;
                            if *age > 14.0 {
                                s.pulled = None;
                                s.grow = 0.0;
                            }
                        }
                        None => {
                            s.grow = (s.grow + dt * 0.08).min(1.0);
                            if s.grow > 0.4 && (s.pos - p.feet).len() < 0.3 {
                                s.pulled = Some(0.0);
                                pulled += 1;
                                fx.burst(
                                    s.pos + s.n * 0.06,
                                    s.n,
                                    Burst {
                                        n: 16,
                                        speed: (0.4, 1.2),
                                        size: (0.025, 0.05),
                                        life: (0.6, 1.2),
                                        cols: &[rgb(0x7CC35A), rgb(0x9ED36E), rgb(0x8A6A4A)],
                                        glow: false,
                                        grav: 3.0,
                                        drag: 1.0,
                                        aim: 0.6,
                                    },
                                );
                                fx.burst(
                                    s.pos + s.n * 0.1,
                                    s.n,
                                    Burst {
                                        n: 12,
                                        speed: (0.3, 0.9),
                                        size: (0.03, 0.06),
                                        life: (0.5, 1.0),
                                        cols: &[rgb(0xFFF1A8), rgb(0xFFFFFF)],
                                        glow: true,
                                        grav: -0.2,
                                        drag: 2.0,
                                        aim: 0.5,
                                    },
                                );
                                fx.ring(s.pos, s.n, 0.45, 0.6, rgb(0xFFE9A8));
                                snd.play(Sfx::Pop, 0.9 + i as f32 * 0.05, 0.6);
                                snd.play(Sfx::Splash, 1.6, 0.15);
                            }
                        }
                    }
                }
                if pulled > 0 {
                    scored |= self.set_score((self.score + pulled).min(self.total));
                }
                if let Some(rose) = &mut self.rose {
                    let d = (rose.pos - p.feet).len();
                    let near = d < 0.7;
                    if near && !rose.near {
                        fx.burst(
                            rose.pos + rose.n * 0.2,
                            rose.n,
                            Burst {
                                n: 14,
                                speed: (0.2, 0.6),
                                size: (0.03, 0.06),
                                life: (0.8, 1.5),
                                cols: &[rgb(0xFF8FA6), rgb(0xFFD0DC)],
                                glow: true,
                                grav: -0.2,
                                drag: 1.5,
                                aim: 0.3,
                            },
                        );
                        snd.play(Sfx::Chime(4), 1.0, 0.35);
                    }
                    rose.near = near;
                    rose.open = approach(rose.open, if near { 1.0 } else { 0.55 }, 2.0, dt);
                    let want =
                        if near { (p.feet - rose.pos).flatten(rose.n).norm() } else { sun.flatten(rose.n).norm() };
                    rose.look = crate::math::approach3(rose.look, want, 2.0, dt).norm();
                }
            }
            Kind::Lamp => {
                if let (Some(lamp), Some(k)) = (&mut self.lamp, &mut self.keeper) {
                    let want = lamp.n.dot(sun) < 0.02;
                    match &mut k.anim {
                        None if want != lamp.lit => k.anim = Some((0.0, want)),
                        Some((u, to)) => {
                            let before = *u;
                            *u += dt;
                            if before < 0.55 && *u >= 0.55 {
                                lamp.lit = *to;
                                let col = if *to { rgb(0xFFD98A) } else { rgb(0xBFC6D6) };
                                fx.burst(
                                    lamp.head,
                                    lamp.n,
                                    Burst {
                                        n: if *to { 26 } else { 10 },
                                        speed: (0.3, 1.0),
                                        size: (0.025, 0.06),
                                        life: (0.5, 1.1),
                                        cols: &[col, lighten(col, 0.5)],
                                        glow: *to,
                                        grav: if *to { 0.6 } else { -0.3 },
                                        drag: 1.6,
                                        aim: 0.2,
                                    },
                                );
                                snd.play(Sfx::Chime(if *to { 3 } else { 1 }), 1.0, 0.45);
                            }
                            if *u > 1.3 {
                                k.anim = None;
                            }
                        }
                        None => {}
                    }
                    let u = k.anim.map_or(2.0, |a| a.0);
                    k.raise = smooth(u / 0.4) * (1.0 - smooth((u - 0.85) / 0.4));
                    lamp.on = approach(lamp.on, if lamp.lit { 1.0 } else { 0.0 }, 5.0, dt);
                    let toward = if (p.feet - k.pos).len() < 1.0 && k.anim.is_none() { p.feet } else { lamp.head };
                    let want = (toward - k.pos).flatten(k.n).norm();
                    k.face = crate::math::approach3(k.face, want, 3.0, dt).flatten(k.n).norm();
                }
                // Count the sunsets the prince sees where he stands.
                let e = up.dot(sun);
                if self.last_e > 0.0 && e <= 0.0 {
                    scored |= self.set_score((self.score + 1).min(self.total));
                    snd.play(Sfx::Chime(0), 0.8, 0.3);
                }
                self.last_e = e;
            }
            Kind::Desert => {
                if let Some(fox) = &mut self.fox {
                    scored |= fox.update(dt, p, fx, snd);
                }
            }
        }
        if self.kind == Kind::Desert {
            let s = self.fox.as_ref().map_or(0, |f| f.trust);
            self.set_score(s);
        }
        // Shooting stars on clear nights.
        let night = self.sky(up, t).stars;
        if night > 0.4 {
            self.shoot_t -= dt;
            if self.shoot_t < 0.0 {
                self.shoot_t = self.rng.range(1.0, 3.0);
                let d = (up * 0.8 + self.rng.dir()).norm();
                let v = any_perp(d).rotate(d, self.rng.range(0.0, TAU)) * 40.0;
                self.shooting.push(Shooting { p: d * 70.0, v, life: 0.9 });
            }
        }
        for s in &mut self.shooting {
            s.life -= dt;
            s.p += s.v * dt;
        }
        self.shooting.retain(|s| s.life > 0.0);
        scored
    }

    /// Stars, the sun and the moon, behind everything (additive). `warp`
    /// stretches the stars into streaks.
    pub fn draw_sky(&self, gl: &Gl, s: &Sprites, cam: V3, t: f32, warp: f32, sky: &Sky) {
        // The sky sits 26 units away, in front of the backdrop at 30: on a
        // 16-bit depth buffer anything much farther rounds to the far plane.
        let k = SKY_D / 70.0;
        let stars = sky.stars.max(warp);
        Sprites::begin(gl.sprite);
        if stars > 0.01 {
            for st in &self.stars {
                let p = cam + st.d * (70.0 * k);
                let tw = 0.55 + 0.45 * (t * (1.5 + st.size) + st.phase).sin();
                let c = rgba(st.col, (stars * tw).min(1.0));
                if warp > 0.02 {
                    s.streak(p, st.d, st.size * k * (1.0 + warp * 20.0), st.size * k * 0.5, c);
                } else {
                    s.billboard(p, st.size * k, c);
                    if st.size > 0.6 {
                        s.billboard(p, st.size * k * 2.6, rgba(st.col, stars * tw * 0.25));
                    }
                }
            }
            for sh in &self.shooting {
                let a = (sh.life / 0.9).min(1.0) * stars;
                s.streak(cam + sh.p.norm() * (70.0 * k), sh.v, 3.5 * k, 0.16 * k, rgba(rgb(0xFFFFFF), a));
            }
        }
        // Cloud banks: soft overlapping puffs, brighter in daylight.
        let day = (1.0 - sky.night) * (1.0 - warp);
        if day > 0.02 {
            for (k, (d, axis, speed, size)) in self.clouds.iter().enumerate() {
                let p = d.rotate(*axis, t * speed) * (self.radius + 3.6);
                let up = p.norm();
                let side = axis.cross(up).norm();
                for i in 0..7 {
                    let f = i as f32;
                    let o = side * ((f - 3.0) * 0.32 * size) + up * ((f * 1.7 + k as f32).sin() * 0.12 * size);
                    let s2 = size * (0.55 + 0.35 * (f * 2.3 + k as f32).sin().abs());
                    s.billboard(p + o, s2, rgba(mix(sky.sun_col, rgb(0xFFFFFF), 0.5), 0.16 * day));
                }
            }
        }
        let sun = cam + sky.sun * (65.0 * k);
        let pulse = 1.0 + 0.03 * (t * 1.5).sin();
        s.billboard(sun, 20.0 * k * pulse, rgba(sky.sun_col, 0.3));
        s.billboard(sun, 6.5 * k, rgba(sky.sun_col, 0.6));
        s.billboard(sun, 3.0 * k, rgba(lighten(sky.sun_col, 0.7), 1.0));
        let moon = cam - sky.sun.rotate(v3(0.0, 0.0, 1.0), 0.5) * (65.0 * k);
        s.billboard(moon, 9.0 * k, rgba(rgb(0xBFD0FF), 0.25 * sky.night));
        s.billboard(moon, 2.4 * k, rgba(rgb(0xF4F1E0), sky.night));
        Sprites::end();
    }

    /// Queues the visible planet tiles front to back for early depth rejection.
    fn queue_tiles(&self, gl: &Gl, cam: V3, fwd: V3) {
        const MAX_HEIGHT: f32 = 4.0;
        const VIEW_DISTANCE: f32 = 60.0;
        let r = self.radius;
        let cam_dist = cam.len().max(r + 0.01);
        let cam_dir = cam * (1.0 / cam_dist);
        let horizon = (r / cam_dist).min(1.0).acos() + (r / (r + MAX_HEIGHT)).min(1.0).acos();
        let mut visible = Vec::with_capacity(self.tiles.len());
        // Preserve the existing tie order for equally distant cells.
        for tile in self.tiles.iter().rev() {
            let theta = tile.dir.dot(cam_dir).clamp(-1.0, 1.0).acos();
            if theta - tile.ang > horizon + 0.05 {
                continue;
            }
            let extent = r * tile.ang;
            let bound = extent + MAX_HEIGHT;
            let delta = tile.dir * r - cam;
            let dist = delta.len();
            if dist - bound < VIEW_DISTANCE && delta.dot(fwd) > -bound {
                visible.push(((dist - extent).max(0.0), tile));
            }
        }
        visible.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (dist, tile) in visible {
            let (mesh, lod) = if dist < CLOSE {
                (tile.close, "close")
            } else if dist < NEAR {
                (tile.near, "near")
            } else if dist < MID {
                (tile.mid, "mid")
            } else {
                (tile.far, "far")
            };
            gl.surface(mesh, lod);
        }
    }

    /// The solid parts of the planet, queued for `Gl::flush`.
    pub fn draw(&self, gl: &Gl, kit: &Kit, t: f32, cam: V3, fwd: V3) {
        if !crate::prof::skip("world") {
            self.queue_tiles(gl, cam, fwd);
        }
        let wind = self.pal.wind;
        let ri = Ink::Radial;
        for v in &self.volcanoes {
            if v.active {
                let x = any_perp(v.n);
                let glow = 1.0 + 0.4 * (t * 3.0 + v.puff).sin();
                gl.part(
                    kit.ball,
                    place(v.top - v.n * 0.004, v.n, x, v3(0.07, 0.008, 0.07)),
                    rgb(0xFF8A3D),
                    glow,
                    Ink::None,
                );
            }
        }
        for s in &self.sprouts {
            let (pos, g, spin) = match s.pulled {
                Some(age) if age < 0.9 => {
                    let u = age / 0.9;
                    (s.pos + s.n * (u * 0.9 - u * u * 0.3), (1.0 - u) * 1.1, u * 9.0)
                }
                Some(_) => continue,
                None => (s.pos, s.grow, 0.0),
            };
            if g < 0.05 {
                continue;
            }
            let bend = wind * ((t * 2.2 + s.phase).sin() * 0.1);
            let dir = (s.n + bend).norm().rotate(any_perp(s.n), spin * 0.3);
            let x = any_perp(dir).rotate(dir, s.phase + spin);
            gl.part(kit.tube, place(pos - s.n * 0.01, dir, x, v3(0.012, 0.11, 0.012) * g), rgb(0x5E9E4A), 0.0, ri);
            let top = pos + dir * (0.1 * g);
            for k in 0..3 {
                let out = x.rotate(dir, k as f32 / 3.0 * TAU);
                let leaf = (out + dir * 0.5).norm();
                gl.part(
                    kit.ball,
                    place(top + leaf * (0.045 * g), leaf, dir, v3(0.022, 0.05, 0.007) * g),
                    rgb(0x7CC35A),
                    0.05,
                    ri,
                );
            }
        }
        if let Some(r) = &self.rose {
            let x = any_perp(r.n);
            gl.part(kit.stub, place(r.pos - r.n * 0.01, r.n, x, v3(0.13, 0.025, 0.13)), rgb(0xC9A55A), 0.0, ri);
            let sway = wind * ((t * 1.6).sin() * 0.05) + r.look * 0.12 * r.open;
            let dir = (r.n + sway).norm();
            gl.part(kit.tube, place(r.pos, dir, x, v3(0.009, 0.2, 0.009)), rgb(0x4E8C42), 0.0, ri);
            for (k, h) in [(0.0f32, 0.08f32), (PI, 0.12)] {
                let out = x.rotate(dir, k + 0.6);
                let leaf = (out + dir * 0.6).norm();
                gl.part(
                    kit.ball,
                    place(r.pos + dir * h + leaf * 0.035, leaf, dir, v3(0.02, 0.04, 0.006)),
                    rgb(0x5FA052),
                    0.0,
                    ri,
                );
            }
            let head = r.pos + dir * 0.22;
            let open = r.open;
            for p in 0..6 {
                let a = p as f32 / 6.0 * TAU + 0.3;
                let out = x.rotate(dir, a);
                let theta = 0.25 + open * 0.75 + if p % 2 == 0 { 0.1 } else { 0.0 };
                let axis = (dir * theta.cos() + out * theta.sin()).norm();
                let normal = axis.cross(dir.cross(out)).norm();
                gl.part(
                    kit.ball,
                    place(head + axis * 0.03, axis, normal, v3(0.028, 0.042, 0.01)),
                    rgb(0xE0444E),
                    0.05,
                    ri,
                );
            }
            gl.part(kit.ball, place(head + dir * 0.012, dir, x, v3(0.027, 0.034, 0.027)), rgb(0xC22F40), 0.05, ri);
        }
        if let Some(l) = &self.lamp {
            let x = any_perp(l.n);
            let iron = rgb(0x34304A);
            gl.part(kit.cone, place(l.head + l.n * 0.055, l.n, x, v3(0.075, 0.07, 0.075)), iron, 0.0, ri);
            gl.part(
                kit.ball,
                place(l.head, l.n, x, v3(0.048, 0.06, 0.048)),
                mix(rgb(0xC9CFE0), rgb(0xFFD98A), l.on),
                l.on * 1.3,
                ri,
            );
            gl.part(kit.stub, place(l.head - l.n * 0.075, l.n, x, v3(0.045, 0.02, 0.045)), iron, 0.0, ri);
        }
        if let Some(k) = &self.keeper {
            self.draw_keeper(gl, kit, k, t);
        }
        if let Some(f) = &self.fox {
            draw_fox(gl, kit, f, ground_r(self.kind, f.n), t);
        }
        if let Some(f) = &self.prop {
            let a = t * 0.9 + (t * 0.3).sin();
            for k in [0.0, PI] {
                let blade = f.x.rotate(f.z, a + k);
                let side = f.y.rotate(f.z, a + k);
                gl.part(
                    kit.cube,
                    crate::math::basis(blade * 0.16, side * 0.025, f.z * 0.01, f.o + blade * 0.16),
                    rgb(0x8A5A3B),
                    0.0,
                    ri,
                );
            }
            gl.part(kit.ball, place(f.o, f.z, f.y, v3(0.035, 0.03, 0.035)), rgb(0xC9473E), 0.0, ri);
        }
    }

    fn keeper_pose(k: &Keeper) -> (Frame, V3, V3) {
        let f = Frame::new(k.pos, k.n, k.face);
        let rest = f.at(v3(0.11, 0.2, 0.06));
        let up = f.at(v3(0.09, 0.42, 0.12));
        let hand = rest.lerp(up, k.raise);
        let pole_dir = f.dir(v3(0.05, 1.0, 0.12).lerp(v3(-0.05, 1.0, 0.55), k.raise)).norm();
        (f, hand, hand + pole_dir * 0.62)
    }

    fn draw_keeper(&self, gl: &Gl, kit: &Kit, k: &Keeper, t: f32) {
        let (f, hand, tip) = World::keeper_pose(k);
        let coat = rgb(0x4A5E9E);
        let navy = rgb(0x2D3561);
        let skin = rgb(0xFFE0C8);
        let ri = Ink::Radial;
        let breathe = 1.0 + (t * 2.0).sin() * 0.015;
        let b = f.scaled(v3(1.0, breathe, 1.0));
        for side in [-1.0f32, 1.0] {
            kit.limb(gl, b.at(v3(side * 0.04, 0.16, 0.0)), f.at(v3(side * 0.045, 0.04, 0.0)), 0.03, navy);
            gl.part(kit.ball, f.part(v3(side * 0.045, 0.03, 0.015), v3(0.038, 0.032, 0.058)), rgb(0x2A2230), 0.0, ri);
        }
        gl.part(kit.stub, b.part(v3(0.0, 0.1, 0.0), v3(0.11, 0.18, 0.1)), coat, 0.0, ri);
        gl.part(kit.ball, b.part(v3(0.0, 0.28, 0.0), v3(0.095, 0.11, 0.085)), coat, 0.0, ri);
        for y in [0.24, 0.3] {
            gl.part(kit.ball, b.part(v3(0.0, y, 0.083), v3(0.012, 0.012, 0.008)), rgb(0xF2C14E), 0.15, Ink::None);
        }
        let shoulder_r = b.at(v3(0.09, 0.34, 0.0));
        kit.limb(gl, shoulder_r, hand, 0.026, coat);
        gl.part(kit.ball, place(hand, k.n, k.face, v3(0.026, 0.026, 0.026)), skin, 0.0, ri);
        kit.limb(gl, b.at(v3(-0.09, 0.34, 0.0)), b.at(v3(-0.12, 0.19, 0.02)), 0.026, coat);
        gl.part(kit.ball, b.part(v3(-0.12, 0.18, 0.02), v3(0.026, 0.026, 0.026)), skin, 0.0, ri);
        let pd = tip - hand;
        gl.part(
            kit.tube,
            place(hand - pd * 0.15, pd, k.face, v3(0.008, pd.len() * 1.15, 0.008)),
            rgb(0x8A5A3B),
            0.0,
            ri,
        );
        let head = b.moved(v3(0.0, 0.45, 0.0)).turned(0, -k.raise * 0.35 + (t * 0.8).sin() * 0.03);
        gl.part(kit.ball, head.part(V3::ZERO, v3(0.1, 0.1, 0.095)), skin, 0.0, ri);
        gl.part(kit.ball, head.part(v3(0.0, -0.01, 0.095), v3(0.024, 0.022, 0.022)), rgb(0xF4A9A0), 0.0, Ink::None);
        gl.part(kit.ball, head.part(v3(0.0, -0.04, 0.085), v3(0.055, 0.014, 0.02)), rgb(0x6B4A3A), 0.0, Ink::None);
        for side in [-1.0f32, 1.0] {
            gl.part(
                kit.ball,
                head.part(v3(side * 0.035, 0.015, 0.088), v3(0.01, 0.014, 0.008)),
                rgb(0x2A2230),
                0.0,
                Ink::None,
            );
        }
        gl.part(kit.stub, head.part(v3(0.0, 0.05, 0.0), v3(0.1, 0.075, 0.1)), navy, 0.0, ri);
        gl.part(kit.ball, head.part(v3(0.0, 0.055, 0.075), v3(0.075, 0.01, 0.05)), rgb(0x232845), 0.0, ri);
    }

    /// The rose's glass globe; after everything solid.
    pub fn draw_glass(&self, gl: &Gl, kit: &Kit) {
        if let Some(r) = &self.rose {
            let x = any_perp(r.n);
            gl.draw_glass(kit.ball, place(r.pos + r.n * 0.13, r.n, x, v3(0.13, 0.18, 0.13)), rgba(rgb(0xDDEEFF), 0.12));
        }
    }

    /// Butterflies near the camera (see-through, inside `gl::alpha`).
    pub fn draw_alpha(&self, gl: &Gl, t: f32, cam: V3) {
        if self.butterflies.is_empty() {
            return;
        }
        Sprites::begin(gl.sprite);
        for (k, (home, phase, col)) in self.butterflies.iter().enumerate() {
            let a = t * 0.35 + phase;
            let drift = home.rotate(any_perp(*home), 0.03 * (a * 1.3).sin()).rotate(*home, a);
            let wobble = (t * 3.1 + phase * 3.0).sin() * 0.06;
            let p = drift * (ground_r(self.kind, drift) + 0.28 + wobble);
            if (p - cam).len() > 7.0 {
                continue;
            }
            let up = drift;
            let heading = any_perp(up).rotate(up, a * 2.0 + k as f32);
            let flap = (t * 17.0 + phase * 5.0).sin();
            let side = up.cross(heading).norm();
            for sgn in [-1.0f32, 1.0] {
                let wing = (side * sgn * (0.3 + 0.7 * flap.abs()) + up * (0.8 * flap.abs())).norm();
                Sprites::quad(p + wing * 0.022, wing * 0.022, heading * 0.018, rgba(*col, 0.95));
            }
        }
        Sprites::end();
    }

    /// Lava, lamp and flame glows (additive).
    pub fn draw_glow(&self, gl: &Gl, s: &Sprites, t: f32) {
        Sprites::begin(gl.sprite);
        for v in self.volcanoes.iter().filter(|v| v.active) {
            let k = 0.8 + 0.2 * (t * 3.0 + v.puff).sin();
            s.billboard(v.top + v.n * 0.03, 0.22 * k, rgba(rgb(0xFF8A3D), 0.45));
        }
        if let Some(r) = &self.rose {
            let k = 0.5 + 0.5 * (t * 2.0).sin();
            s.billboard(
                r.pos + r.n * 0.18,
                0.12 + 0.05 * k,
                rgba(rgb(0xFFB0C0), 0.15 + 0.2 * (r.open - 0.55).max(0.0) * 2.0),
            );
        }
        if let Some(l) = &self.lamp {
            if l.on > 0.01 {
                let f = 0.95 + 0.05 * (t * 13.0).sin();
                s.billboard(l.head, 0.75 * f * l.on, rgba(rgb(0xFFC870), 0.5 * l.on));
                s.billboard(l.head, 0.2 * f, rgba(rgb(0xFFF4D8), 0.9 * l.on));
            }
        }
        if let Some(k) = &self.keeper {
            if k.raise > 0.05 {
                let (_, _, tip) = World::keeper_pose(k);
                let f = 0.9 + 0.1 * (t * 20.0).sin();
                s.billboard(tip, 0.09 * f * k.raise, rgba(rgb(0xFFB45C), k.raise));
                s.billboard(tip, 0.035, rgba(rgb(0xFFF4D8), k.raise));
            }
        }
        Sprites::end();
    }
}

fn draw_fox(gl: &Gl, kit: &Kit, f: &Fox, r: f32, t: f32) {
    let orange = rgb(0xE8873A);
    let cream = rgb(0xFFF1DE);
    let dark = rgb(0x5A3222);
    let ri = Ink::Radial;
    let run = (f.speed / 1.5).min(1.0);
    let hop = (f.phase.sin().abs() * 0.05) * run;
    let g = Frame::new(f.n * (r + hop), f.n, f.face);
    // Sitting tips the body up at the front.
    let b = g.turned(0, -0.55 * f.sit);
    for (k, (x, z)) in [(-1.0f32, 1.0f32), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)].into_iter().enumerate() {
        let ph = f.phase + if k % 3 == 0 { 0.0 } else { PI };
        let swing = ph.sin() * 0.06 * run;
        let hip = b.at(v3(x * 0.045, 0.13, z * 0.09));
        let foot = if z < 0.0 && f.sit > 0.5 {
            g.at(v3(x * 0.06, 0.02, -0.02))
        } else {
            g.at(v3(x * 0.045, 0.02, z * 0.09 * (1.0 - f.sit * 0.3) + swing))
        };
        kit.limb(gl, hip, foot, 0.022, orange);
        gl.part(kit.ball, place(foot, f.n, f.face, v3(0.022, 0.018, 0.03)), dark, 0.0, ri);
    }
    gl.part(kit.ball, b.part(v3(0.0, 0.15, 0.0), v3(0.07, 0.07, 0.13)), orange, 0.0, ri);
    gl.part(kit.ball, b.part(v3(0.0, 0.15, 0.085), v3(0.055, 0.06, 0.05)), cream, 0.0, ri);
    let look = (t * 0.7).sin() * 0.3 * (1.0 - run);
    let tilt = (t * 1.1).sin() * 0.2 * f.sit;
    let head = b.moved(v3(0.0, 0.25, 0.13)).turned(0, 0.55 * f.sit).turned(1, look).turned(2, tilt);
    gl.part(kit.ball, head.part(V3::ZERO, v3(0.075, 0.068, 0.07)), orange, 0.0, ri);
    gl.part(kit.ball, head.part(v3(0.0, -0.02, 0.04), v3(0.05, 0.035, 0.04)), cream, 0.0, Ink::None);
    gl.part(kit.cone, place(head.at(v3(0.0, -0.01, 0.05)), head.z, head.y, v3(0.034, 0.08, 0.026)), orange, 0.0, ri);
    gl.part(kit.ball, head.part(v3(0.0, -0.008, 0.13), v3(0.014, 0.012, 0.012)), rgb(0x2A1A18), 0.0, Ink::None);
    for side in [-1.0f32, 1.0] {
        let ear = head.dir(v3(side * 0.45, 1.0, -0.1) + v3(0.0, 0.0, (t * 3.0 + side).sin() * 0.08)).norm();
        gl.part(
            kit.cone,
            place(head.at(v3(side * 0.04, 0.04, -0.01)), ear, head.z, v3(0.03, 0.085, 0.016)),
            orange,
            0.0,
            ri,
        );
        gl.part(
            kit.cone,
            place(head.at(v3(side * 0.04, 0.045, 0.002)), ear, head.z, v3(0.017, 0.06, 0.008)),
            dark,
            0.0,
            Ink::None,
        );
        gl.part(
            kit.ball,
            head.part(v3(side * 0.03, 0.012, 0.062), v3(0.01, 0.015, 0.008)),
            rgb(0x2A1A18),
            0.0,
            Ink::None,
        );
    }
    // The tail: a curve of balls, bushy at the end with a white tip.
    let wag = f.wag.sin() * 0.6;
    let mut p = b.at(v3(0.0, 0.16, -0.12));
    let mut d = b.dir(v3(0.0, 0.5, -1.0)).norm();
    for k in 0..5 {
        let kf = k as f32;
        d = d.rotate(b.y, wag * 0.25).rotate(b.x, -0.12 * (1.0 + f.sit)).norm();
        p += d * 0.045;
        let s = 0.035 + kf * 0.007;
        let c = if k == 4 { cream } else { orange };
        gl.part(kit.ball, place(p, d, b.y, v3(s, s * 1.3, s)), c, 0.0, ri);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_are_ahead_of_the_spawn() {
        let n = spot(0.4, 0.0);
        assert!(n.z > 0.3 && n.y > 0.8, "{n:?}");
    }

    #[test]
    fn the_fox_comes_closer_as_it_trusts() {
        assert!(Fox::preferred_distance(4) < Fox::preferred_distance(0));
        assert!(Fox::preferred_distance(FOX_TRUST) > 0.8);
    }

    #[test]
    fn the_fox_reaches_full_trust_while_the_player_waits_at_spawn() {
        let spawn = V3::Y;
        let player = PState { feet: spawn * ground_r(Kind::Desert, spawn), face: any_perp(spawn), speed: 0.0 };
        let mut fox = Fox::new(spawn);
        let mut fx = Fx::new();
        let mut snd = Sounds::new(false);
        for _ in 0..(20 * 60) {
            fox.update(1.0 / 60.0, &player, &mut fx, &mut snd);
        }
        assert_eq!(fox.trust, FOX_TRUST);
    }
}
