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
use crate::mesh::{self, no_sway, Builder, Group, Kit, Tile};
use crate::sound::{Sfx, Sounds};
use crate::text::T;
use crate::trouble::{self, Caterpillar, Cinder, Devil, Dust, Moth, PlanePart};
use raylib_sys::{Color, Mesh};
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    B612,
    Lamp,
    Desert,
}

pub const KINDS: [Kind; 3] = [Kind::B612, Kind::Lamp, Kind::Desert];

impl Kind {
    /// The planet's mean radius in world units. The book's lamplighter has
    /// the smallest planet and Earth is by far the largest.
    pub const fn radius(self) -> f32 {
        match self {
            Kind::B612 => 18.0,
            Kind::Lamp => 15.0,
            Kind::Desert => 25.0,
        }
    }

    /// Seconds from one sunrise to the next. The lamplighter's planet turns
    /// so fast that he lights and puts out his lamp every few seconds.
    pub const fn day(self) -> f32 {
        match self {
            Kind::B612 => 240.0,
            Kind::Lamp => 14.0,
            Kind::Desert => 200.0,
        }
    }
}

const FOX_TRUST: usize = 5;
/// Snuffer moths to shoo away from the lamp.
const MOTHS: usize = 6;
/// The sandstorm's cycle in seconds, and how long it stays calm first.
const STORM_CYCLE: f32 = 52.0;
const STORM_CALM: f32 = 30.0;

/// The sun's height before the prince has seen it, on arriving or loading.
const UNSEEN: f32 = 0.0;

/// Whether the sun went down between the last frame's height above the
/// prince, `before`, and this one's, `now`. After `UNSEEN` nothing counts.
fn sets(before: f32, now: f32) -> bool {
    before > 0.0 && now <= 0.0
}
/// Up to `CLOSE` from the camera a cell has all its grass, flowers and full
/// scenery; up to `NEAR` half the grass; up to `MID` only coarser scenery;
/// beyond, only the ground. Groups of four cells all beyond `GROUP` are
/// drawn as one.
const CLOSE: f32 = 3.0;
const NEAR: f32 = 6.0;
const GROUP: f32 = 9.0;
const MID: f32 = 10.0;
/// The nearest the stars, sun and moon are drawn to the camera; farther
/// when the camera is high enough to see more of the planet than that.
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
    pub grounded: bool,
    /// Whether the prince came down from a jump this frame.
    pub landed: bool,
}

/// Where autoplay should walk: a point, how near to stop, whether to stand
/// still once there, and whether to jump on it.
pub struct Goal {
    pub at: V3,
    pub arrive: f32,
    pub still: bool,
    pub hop: bool,
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
    // Crests about five units apart, bending across the planet.
    let a = n.dot(v3(0.8, 0.2, 0.55)) * 32.0 + 2.2 * (n.dot(v3(-0.3, 0.9, 0.3)) * 9.0).sin();
    let ridge = 1.0 - a.sin().abs();
    let swell = 0.9 * bumps(n * 1.5, 5.0) + 0.3 * bumps(n * 5.0, 1.1);
    // Low ground has gentler crests, so the plain round the spawn stays walkable.
    (0.35 + 0.15 * swell) * ridge * ridge - 0.15 + swell
}

fn ground_r(kind: Kind, n: V3) -> f32 {
    let r = kind.radius();
    match kind {
        Kind::B612 => {
            r + 1.1 * bumps(n, 1.7)
                + 0.45 * bumps(n * 3.0, 4.1)
                + 0.16 * bumps(n * 8.0, 2.2)
                + 0.05 * bumps(n * 18.0, 0.7)
        }
        Kind::Lamp => r + 0.45 * bumps(n, 3.0) + 0.18 * bumps(n * 4.0, 1.3) + 0.04 * bumps(n * 13.0, 2.0),
        Kind::Desert => r + dune(n),
    }
}

fn ground_colour(kind: Kind, n: V3, h: f32) -> (Color, f32) {
    let r = kind.radius();
    match kind {
        Kind::B612 => {
            let k = ((h - r) / 1.2 * 0.5 + 0.5).clamp(0.0, 1.0);
            let mut c = mix(rgb(0x557F35), rgb(0x86AE4E), k);
            c = mix(c, rgb(0xA5B457), smooth((bumps(n * 4.0, 6.0) - 0.3) / 0.4) * 0.5);
            c = mix(c, rgb(0x46702E), smooth((bumps(n * 10.0, 3.0) - 0.4) / 0.3) * 0.4);
            let d = path_dist(n);
            c = mix(rgb(0xB89D74), c, smooth((d - 0.16) / 0.14));
            (c, 0.0)
        }
        Kind::Lamp => {
            let k = bumps(n * 6.0, 9.0) * 0.5 + 0.5;
            let c = mix(rgb(0xB7A49A), rgb(0xD8C7B4), k);
            let c = mix(c, rgb(0x9C8C86), smooth((h - r + 0.2) / -0.4) * 0.5);
            (mix(c, rgb(0xA8B488), smooth((bumps(n * 3.0, 2.0) - 0.5) / 0.3) * 0.6), 0.0)
        }
        Kind::Desert => {
            let k = ((h - r + 0.6) / 1.6).clamp(0.0, 1.0);
            let c = mix(rgb(0xD5A472), rgb(0xF3D8A4), k);
            let ripple = (n.dot(v3(0.3, 0.2, 0.93)) * 220.0 + bumps(n * 4.0, 2.0) * 3.0).sin();
            (gl::scale(c, 1.0 + ripple * 0.03), 0.0)
        }
    }
}

/// A direction `f` radians ahead of the spawn (towards +Z) and `s` to the
/// side.
fn spot(f: f32, s: f32) -> V3 {
    V3::Y.rotate(v3(1.0, 0.0, 0.0), f).rotate(v3(0.0, 0.0, 1.0), -s).norm()
}

/// A direction `f` units ahead of the spawn and `s` to the side, walking
/// over a planet of radius `r`.
fn spot_at(r: f32, f: f32, s: f32) -> V3 {
    spot(f / r, s / r)
}

/// Picks `count` directions at least `sep` radians from each other and from
/// `taken`.
/// Which of `active` volcanoes are swept: those whose bit is set in
/// `swept`, or for saves that only kept a count, the first `count`.
fn mark_swept(active: usize, count: usize, swept: Option<u8>) -> Vec<bool> {
    (0..active).map(|i| swept.map_or(i < count, |s| s & 1 << i != 0)).collect()
}

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

/// A steady pseudo-random number in 0..1 for a direction, for thinning out
/// scenery without a random number generator.
fn hash01(d: V3) -> f32 {
    ((d.x * 127.1 + d.y * 311.7 + d.z * 74.7).sin() * 43758.547).fract().abs()
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
            b612_spot(-0.28, -0.12),
            b612_spot(-0.1, -0.02),
            b612_spot(0.06, 0.0),
            b612_spot(0.18, -0.08),
            b612_spot(0.27, -0.06),
            b612_spot(0.45, -0.12),
            b612_spot(0.6, 0.08),
            b612_spot(0.68, 0.3),
            b612_spot(0.6, 0.55),
            b612_spot(0.4, 0.58),
            b612_spot(0.22, 0.42),
            b612_spot(0.08, 0.26),
            b612_spot(-0.04, 0.12),
            b612_spot(-0.1, -0.02),
        ];
        let mut out = Vec::new();
        for w in way.windows(2) {
            let (a, b) = (w[0], w[1]);
            let side = a.cross(b).norm();
            for i in 0..24 {
                let t = i as f32 / 24.0;
                let wig = (t * TAU + a.x * 13.0).sin() * 0.012 * B612_SPREAD;
                out.push((a.lerp(b, t).norm() + side * wig).norm());
            }
        }
        out
    })
}

/// Distance from `n` to the path, in world units on B612.
fn path_dist(n: V3) -> f32 {
    b612_path().iter().map(|p| (n - *p).len()).fold(f32::MAX, f32::min) * Kind::B612.radius()
}

/// B612 was laid out in radians on a planet of radius 9; on the bigger planet
/// the same plan spreads half as far again in world units.
const B612_SPREAD: f32 = 9.0 * 1.5 / Kind::B612.radius();

/// A direction on B612's plan, `f` and `s` in its original radians.
fn b612_spot(f: f32, s: f32) -> V3 {
    spot(f * B612_SPREAD, s * B612_SPREAD)
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
    /// Builds up to 1, when it throws cinders; sweeping it sets it back.
    pressure: f32,
    /// Seconds of sweeping so far.
    sweep: f32,
    /// Whether it has been swept on this journey, for the task.
    swept: bool,
}

/// Seconds of standing by a volcano to sweep it.
const SWEEP: f32 = 2.5;
/// How many sprouts B612 can hold before they stop spreading.
const MAX_SPROUTS: usize = 14;

struct Sprout {
    n: V3,
    pos: V3,
    grow: f32,
    /// How grown it was when the planet was made, to start a journey over.
    grow0: f32,
    pulled: Option<f32>,
    phase: f32,
    /// Seconds fully grown: a sprout left alone long enough seeds another.
    seed: f32,
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
    /// Seconds the lamplighter stares at his snuffed lamp before lighting it again.
    out: f32,
}

struct Keeper {
    n: V3,
    pos: V3,
    face: V3,
    /// The lighting or snuffing in progress: seconds in, and the new state.
    anim: Option<(f32, bool)>,
    raise: f32,
    /// Where his model's hand is this frame, when he is drawn from one.
    hand: Option<V3>,
}

/// A character drawn from a model: where it stands, how fast it moves and
/// whether it is busy (the lamplighter at his lamp, the fox sitting).
pub struct Actor {
    pub frame: Frame,
    pub speed: f32,
    pub busy: bool,
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
        let n = spot_at(Kind::Desert.radius(), 3.8, 0.9);
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
    /// Once tamed it follows the prince, or trots to `lead` to show him
    /// something buried there.
    fn update(&mut self, dt: f32, p: &PState, lead: Option<V3>, fx: &mut Fx, snd: &mut Sounds) -> bool {
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
            let spot = lead.unwrap_or(p.feet - p.face * 0.55 + right * 0.4);
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
    /// The distance from the centre to the highest point of the ground or scenery.
    top: f32,
    /// The distance from the centre to the lowest ground.
    floor: f32,
    pal: Pal,
    sun0: V3,
    period: f32,
    pub spawn: V3,
    tiles: Vec<Tile>,
    groups: Vec<Group>,
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
    /// Sprouts pulled up in the last update: the score stops at `total`,
    /// but pulled sprouts grow back and can be pulled again.
    pub pulled: usize,
    pub pop: f32,
    moths: Vec<Moth>,
    moth_t: f32,
    devils: Vec<Devil>,
    plane_parts: Vec<PlanePart>,
    /// How strong the sandstorm is now, 0 to 1, and seconds into its cycle.
    pub storm: f32,
    storm_t: f32,
    /// Whether the lamplighter and the fox are drawn from models by the app
    /// rather than built from shapes here.
    pub models: bool,
    /// The task's parts: done so far, needed and what they are. `score`
    /// and `total` are their sums.
    pub parts: [usize; 3],
    pub needs: [usize; 3],
    pub labels: [T; 3],
    dust: Dust,
    caterpillars: Vec<Caterpillar>,
    cinders: Vec<Cinder>,
    /// The sprouts the planet starts with; those that spread are let go
    /// when it starts over.
    first_sprouts: usize,
    /// Set by each update: stardust gathered, troubles seen to, and where a
    /// trouble knocked the prince back from.
    pub gathered: u32,
    pub handled: u32,
    pub bump: Option<V3>,
    bump_cool: f32,
    /// The planet in miniature, ground and coarse scenery in one mesh, for
    /// the storybook: in colour, and as a blue ink sketch.
    pub mini: [Mesh; 2],
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
                kind.day(),
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
                kind.day(),
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
                kind.day(),
            ),
        };
        let spawn = V3::Y;
        let r = move |n: V3| ground_r(kind, n);
        let ground = mesh::planet(mesh::GROUND_SUBDIV, &r, &|n, h| ground_colour(kind, n, h));
        let coarse_ground = mesh::planet(mesh::COARSE_SUBDIV, &r, &|n, h| ground_colour(kind, n, h));
        let mut grass;
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
            top: 0.0,
            floor: 0.0,
            pal,
            sun0,
            period,
            spawn,
            tiles: Vec::new(),
            groups: Vec::new(),
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
            last_e: UNSEEN,
            rng: Rng::new(kind as u64 + 3),
            score: 0,
            total: 0,
            pulled: 0,
            pop: 0.0,
            moths: Vec::new(),
            moth_t: 0.0,
            devils: Vec::new(),
            plane_parts: Vec::new(),
            storm: 0.0,
            storm_t: 0.0,
            models: false,
            parts: [0; 3],
            needs: [0; 3],
            labels: [T::Sprouts; 3],
            dust: Dust::new(&[], 0),
            caterpillars: Vec::new(),
            cinders: Vec::new(),
            first_sprouts: 0,
            gathered: 0,
            handled: 0,
            bump: None,
            bump_cool: 0.0,
            mini: unsafe { std::mem::zeroed() },
        };
        let mut props = Builder::default();
        props.lo = Some(Box::default());
        let mut decor = Builder::default();
        let mut taken = vec![spawn];
        // Distances below are in world units: `ang` turns them into radians
        // on this planet and `within` into the cosine of that angle.
        let rad = kind.radius();
        let ang = move |u: f32| u / rad;
        let within = move |u: f32| (u / rad).cos();
        match kind {
            Kind::B612 => {
                let r = &r;
                let off_path = |d: V3, w: f32| path_dist(d) > w;
                // Two smoking volcanoes and an extinct one, as in the book.
                for (i, n) in
                    [b612_spot(0.3, -0.2), b612_spot(0.37, -0.07), b612_spot(0.33, 0.3)].into_iter().enumerate()
                {
                    taken.push(n);
                    let active = i < 2;
                    let k = 2.6;
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
                    w.volcanoes.push(Volcano {
                        top: base + n * (0.23 * k),
                        n,
                        active,
                        puff: rng.f(),
                        pressure: rng.range(0.0, 0.4),
                        sweep: 0.0,
                        swept: false,
                    });
                }
                let rn = b612_spot(0.11, 0.06);
                taken.push(rn);
                w.rose = Some(Rose { n: rn, pos: rn * r(rn), open: 0.6, look: any_perp(rn), near: false });
                // A chair for watching sunsets, turned towards the sun.
                let cn = b612_spot(-0.08, 0.14);
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
                // The baobab that was not pulled up in time, a landmark on the
                // far hill, and two more over the horizon to walk towards.
                for bn in [b612_spot(0.72, 0.38), spot(1.75, -0.7), spot(-1.3, 1.25)] {
                    taken.push(bn);
                    baobab(&mut props, bn, r(bn), &mut rng);
                }
                // A wooden fence along the path by the rose.
                let path = b612_path();
                for w2 in path[30..58].windows(2).step_by(3) {
                    let (a, b) = (w2[0], w2[1]);
                    let side = a.cross(b - a).norm() * (0.36 / rad);
                    let p0 = (a + side).norm();
                    fence_post(&mut props, p0, r(p0));
                }
                for k in (30..55).step_by(3) {
                    let (a, b) = (path[k], path[k + 3]);
                    let side = a.cross(b - a).norm() * (0.36 / rad);
                    let (p0, p1) = ((a + side).norm(), (b + side).norm());
                    for h in [0.11f32, 0.21] {
                        let (q0, q1) = (p0 * (r(p0) + h), p1 * (r(p1) + h));
                        let d = q1 - q0;
                        let up = p0.lerp(p1, 0.5).norm();
                        let x = d.cross(up).norm();
                        mesh::cuboid(&mut props, (q0 + q1) * 0.5, [d * 0.5, up * 0.014, x * 0.012], rgb(0xA8754C));
                    }
                }
                for n in
                    scatter(&mut rng, 7, ang(0.8), &mut taken, &|d| d.dot(V3::Y) > within(6.0) && off_path(d, 0.35))
                {
                    let grow = rng.range(0.6, 1.0);
                    w.sprouts.push(Sprout {
                        n,
                        pos: n * r(n),
                        grow,
                        grow0: grow,
                        pulled: None,
                        phase: rng.f() * TAU,
                        seed: 0.0,
                    });
                }
                // Woods and meadows thin out away from the path, so the far
                // side of the planet stays open and cheap to draw.
                let lush = |d: V3| 1.0 - 0.65 * smooth((path_dist(d) - 5.0) / 9.0);
                for n in scatter(&mut rng, 80, ang(1.4), &mut taken, &|d| {
                    off_path(d, 0.9) && d.dot(V3::Y) < within(2.5) && hash01(d) < lush(d)
                }) {
                    tree(&mut props, Kind::B612, n, r(n), &mut rng);
                }
                for n in scatter(&mut rng, 180, ang(0.5), &mut taken, &|d| off_path(d, 0.5) && hash01(d) < lush(d)) {
                    bush(&mut props, n, r(n), &mut rng);
                }
                for n in scatter(&mut rng, 360, ang(0.36), &mut taken, &|d| off_path(d, 0.35)) {
                    let size = if rng.f() < 0.2 { rng.range(0.25, 0.5) } else { rng.range(0.06, 0.18) };
                    mossy_rock(&mut props, n, r(n), size, &mut rng);
                }
                // Patches of flowers: daisies, pink, yellow and lilac.
                let flowers =
                    [rgb(0xFFFFFF), rgb(0xFFF6E0), rgb(0xFFB3C8), rgb(0xFFE070), rgb(0xC6A8F0), rgb(0xFF8FA3)];
                let gap = within(0.25);
                let spread = ang(0.22);
                let mut placed = 0;
                while placed < 1400 {
                    let c = rng.dir();
                    if !off_path(c, 0.35) || taken.iter().any(|t| t.dot(c) > gap) || hash01(c) > lush(c) {
                        continue;
                    }
                    placed += 1;
                    let col = flowers[(rng.next() % flowers.len() as u64) as usize];
                    let t1 = any_perp(c);
                    let t2 = c.cross(t1);
                    for _ in 0..(2 + rng.next() % 5) {
                        let n = (c + t1 * rng.range(-spread, spread) + t2 * rng.range(-spread, spread)).norm();
                        flower(&mut decor, n, r(n), col, &mut rng);
                    }
                }
                let bare = taken.clone();
                let clear = within(0.16);
                let keep =
                    move |d: V3| path_dist(d) > 0.28 && hash01(d) < lush(d) && bare.iter().all(|t| t.dot(d) < clear);
                grass = mesh::grass(
                    &mut rng,
                    30000,
                    r,
                    &keep,
                    (0.09, 0.21),
                    &[rgb(0x5A8C3A), rgb(0x6C9C42), rgb(0x7FAD4C), rgb(0x93BB58), rgb(0x4D7A33), rgb(0xA3B95A)],
                );
                for _ in 0..30 {
                    let d = rng.dir();
                    w.clouds.push((d, any_perp(d), rng.range(0.004, 0.01), rng.range(1.2, 2.2)));
                }
                for _ in 0..40 {
                    let d = (V3::Y + rng.dir() * 0.25).norm();
                    let c = [rgb(0xFFF3A8), rgb(0xFFFFFF), rgb(0xFFB0C8), rgb(0xA8D8FF), rgb(0xFFC070)]
                        [(rng.next() % 5) as usize];
                    w.butterflies.push((d, rng.f() * TAU, c));
                }
                w.first_sprouts = w.sprouts.len();
                // Caterpillars come over the hills, a little way from the rose.
                let rose = w.rose.as_ref().map_or(spawn, |r| r.n);
                for i in 0..3 {
                    let axis = any_perp(rose).rotate(rose, 0.4 + i as f32 * TAU / 3.0);
                    w.caterpillars.push(Caterpillar::new(rose.rotate(axis, ang(6.0)).norm(), i as f32 * 2.1));
                }
                w.needs = [10, 2, 3];
                w.labels = [T::Sprouts, T::Volcanoes, T::Caterpillars];
            }
            Kind::Lamp => {
                let ln = spot_at(rad, 2.6, 0.3);
                taken.push(ln);
                let base = ln * (r(ln) - 0.02);
                let iron = rgb(0x34304A);
                mesh::frustum(&mut props, base, ln, 12, (0.0, 0.07, 0.08, 0.045), iron, &no_sway, true);
                mesh::frustum(&mut props, base, ln, 10, (0.0, 0.02, 0.84, 0.016), iron, &no_sway, true);
                mesh::frustum(&mut props, base + ln * 0.84, ln, 10, (0.0, 0.03, 0.03, 0.03), iron, &no_sway, true);
                w.lamp = Some(LampPost { n: ln, head: base + ln * 0.93, lit: false, on: 0.0, out: 0.0 });
                let kn = ln.rotate(any_perp(ln).rotate(ln, 1.2), ang(0.25));
                taken.push(kn);
                w.keeper = Some(Keeper {
                    n: kn,
                    pos: kn * r(kn),
                    face: (base - kn * r(kn)).flatten(kn).norm(),
                    anim: None,
                    raise: 0.0,
                    hand: None,
                });
                // A bare, stony little world: boulders, pebbles, and a few
                // flowers and tufts sheltering between them.
                for n in scatter(&mut rng, 40, ang(2.5), &mut taken, &|d| d.dot(ln) < within(1.5)) {
                    rock(&mut props, n, r(n), rgb(0x9C8E88), rng.range(0.25, 0.6), &mut rng);
                }
                for n in scatter(&mut rng, 260, ang(0.6), &mut taken, &|d| d.dot(ln) < within(1.0)) {
                    rock(&mut props, n, r(n), rgb(0xA89A92), rng.range(0.04, 0.14), &mut rng);
                }
                let gap = within(0.17);
                let mut placed = 0;
                while placed < 900 {
                    let n = rng.dir();
                    if taken.iter().all(|t| t.dot(n) < gap) && hash01(n * 3.0) < 0.5 {
                        placed += 1;
                        let c = [rgb(0xFFFFFF), rgb(0xFFB8C8), rgb(0xFFE27A)][(rng.next() % 3) as usize];
                        flower(&mut decor, n, r(n), c, &mut rng);
                    }
                }
                let bare = taken.clone();
                grass = mesh::grass(
                    &mut rng,
                    9000,
                    &r,
                    &|d| bare.iter().all(|t| t.dot(d) < gap) && hash01(d * 3.0) < 0.5,
                    (0.04, 0.1),
                    &[rgb(0x8FB070), rgb(0xA0BE7A)],
                );
                // Three sunsets, and the moths kept from the lamp at night.
                w.needs = [3, MOTHS, 0];
                w.labels = [T::Sunsets, T::Moths, T::Sunsets];
            }
            Kind::Desert => {
                // The biplane, nose down in the sand.
                let pn = spot_at(rad, 2.0, -1.3);
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
                // Four parts thrown from the aeroplane: two lie in the open,
                // two the sand has covered, for the fox to find.
                for (shape, (f, s, buried)) in
                    [(3.4, -2.6, false), (0.4, -3.4, false), (9.5, 3.0, true), (-6.5, 4.5, true)]
                        .into_iter()
                        .enumerate()
                {
                    let n = spot_at(rad, f, s);
                    taken.push(n);
                    w.plane_parts.push(PlanePart { n, shape, buried, found: false, shown: 0.0 });
                }
                let wheat = spot_at(rad, 0.7, 2.2);
                let field_edge = within(1.6);
                let off_field = within(2.4);
                for n in scatter(&mut rng, 30, ang(3.0), &mut taken, &|d| d.dot(wheat) < off_field) {
                    rock(&mut props, n, r(n), rgb(0xB08A70), rng.range(0.3, 0.7), &mut rng);
                }
                for n in scatter(&mut rng, 90, ang(1.2), &mut taken, &|d| d.dot(wheat) < off_field) {
                    rock(&mut props, n, r(n), rgb(0xB9937A), 0.12, &mut rng);
                }
                for n in scatter(&mut rng, 140, ang(0.8), &mut taken, &|d| d.dot(wheat) < off_field) {
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
                let clear = within(0.15);
                let field = move |d: V3| d.dot(wheat) > field_edge && bare.iter().all(|t| t.dot(d) < clear);
                grass = mesh::grass_in(
                    &mut rng,
                    3500,
                    (wheat, ang(1.6)),
                    &r,
                    &field,
                    (0.1, 0.22),
                    &[rgb(0xC99E48), rgb(0xB68C3C), rgb(0xD6B25A)],
                );
                // Dry tufts here and there over the dunes.
                let bare = taken.clone();
                let tufts = move |d: V3| {
                    d.dot(wheat) < off_field && hash01(d * 2.0) < 0.12 && bare.iter().all(|t| t.dot(d) < clear)
                };
                grass.append(&mesh::grass(
                    &mut rng,
                    2400,
                    &r,
                    &tufts,
                    (0.06, 0.14),
                    &[rgb(0xB59A5A), rgb(0xA08850), rgb(0xC2AE70)],
                ));
                w.needs = [FOX_TRUST, 4, 0];
                w.labels = [T::Tamed, T::PlaneParts, T::Tamed];
            }
        }
        if w.needs == [0; 3] {
            w.needs = [w.total, 0, 0];
            w.labels = [w.name().2; 3];
        }
        w.total = w.needs.iter().sum();
        // Stardust over the whole planet.
        let spots = scatter(&mut rng, 12, ang(2.2), &mut taken, &|_| true);
        w.dust = Dust::new(&spots, kind as u64 + 17);
        let lo = props.lo.take().map(|b| *b).unwrap_or_default();
        let mut mini = coarse_ground.clone();
        mini.append(&lo);
        // The book shows a planet at most about 160 pixels across.
        let mini = mini.clustered(rad * 0.025);
        if std::env::var("LITTLEPRINCE_PROFILE").is_ok() {
            eprintln!("prof: {kind:?} in miniature: {} triangles", mini.tris());
        }
        let sketch = mini.recoloured(|c| {
            let l = (0.3 * c.r as f32 + 0.55 * c.g as f32 + 0.15 * c.b as f32) / 255.0;
            mix(rgb(0x41509A), rgb(0xE8ECF8), smooth((l - 0.3) / 0.55))
        });
        w.mini = [mini.upload_static(), sketch.upload_static()];
        (w.tiles, w.groups) = mesh::tiles(
            [ground, props, lo, decor, grass],
            coarse_ground,
            mesh::TileLayers {
                props: !crate::prof::skip("props"),
                flowers: !crate::prof::skip("flowers"),
                grass: !crate::prof::skip("grass"),
            },
        );
        w.top = w.tiles.iter().map(|t| t.top).fold(w.radius, f32::max);
        w.floor = w.tiles.iter().map(|t| t.low).fold(w.radius, f32::min);
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

    /// How far from `cam` the sky goes: beyond everything on the planet the
    /// camera can see, over the horizon included, and never nearer than
    /// `SKY_D`. Kept as near as it can be, for the handheld's 16-bit depth
    /// buffer.
    pub fn sky_distance(&self, cam: V3) -> f32 {
        let r = self.radius;
        let to_horizon = (cam.dot(cam) - r * r).max(0.0).sqrt();
        let beyond = (self.top * self.top - r * r).max(0.0).sqrt();
        (to_horizon + beyond + 1.0).max(SKY_D)
    }

    /// The lamp's light, if there is one and it is lit.
    pub fn lamp_light(&self) -> Option<(V3, f32)> {
        self.lamp.as_ref().filter(|l| l.on > 0.01).map(|l| (l.head, l.on))
    }

    /// Where autoplay should walk: the task's parts in turn, then the
    /// character who set it.
    pub fn autoplay_target(&self, from: V3) -> Option<Goal> {
        let kind = self.kind;
        let on = |n: V3| n * ground_r(kind, n);
        let walk = |at: V3| Goal { at, arrive: 0.0, still: false, hop: false };
        let dist = |a: &V3, b: &V3| (*a - from).len().total_cmp(&(*b - from).len());
        // Stardust lying close by is picked up on the way, except while
        // keeping still for the fox.
        if self.kind != Kind::Desert {
            if let Some(at) = self.dust.nearest(from, 2.5, &|n| ground_r(kind, n)) {
                return Some(walk(at));
            }
        }
        match self.kind {
            Kind::B612 => {
                let crawling = self.caterpillars.iter().filter(|c| c.crawling()).map(|c| on(c.n)).min_by(dist);
                let ready =
                    self.sprouts.iter().filter(|s| s.pulled.is_none() && s.grow > 0.55).map(|s| s.pos).min_by(dist);
                if let Some(at) = crawling.filter(|_| self.parts[2] < self.needs[2]) {
                    return Some(Goal { at, arrive: 0.0, still: false, hop: true });
                }
                if let Some(at) = ready.filter(|_| self.parts[0] < self.needs[0]) {
                    return Some(walk(at));
                }
                if self.parts[1] < self.needs[1] {
                    if let Some(v) = self.volcanoes.iter().find(|v| v.active && !v.swept) {
                        let to = (from.norm() - v.n).flatten(v.n);
                        let to = if to.len() < 1e-4 { any_perp(v.n) } else { to.norm() };
                        let spot = v.n.rotate(v.n.cross(to).norm(), 0.75 / kind.radius());
                        return Some(Goal { at: on(spot), arrive: 0.25, still: true, hop: false });
                    }
                }
                match ready {
                    Some(at) => Some(walk(at)),
                    None => self.rose.as_ref().map(|r| Goal { at: r.pos, arrive: 0.55, still: true, hop: false }),
                }
            }
            Kind::Lamp => {
                let moth = self.moths.iter().filter(|m| m.coming()).map(|m| on(m.n)).min_by(dist);
                if let Some(at) = moth {
                    return Some(Goal { at, arrive: 0.0, still: false, hop: true });
                }
                // Keep watch by the lamp until the task is done.
                let lamp = self.lamp.as_ref().filter(|_| !self.ready()).map(|l| on(l.n));
                lamp.map(|at| Goal { at, arrive: 1.3, still: true, hop: false })
            }
            Kind::Desert => {
                if let Some(f) = self.fox.as_ref().filter(|f| f.trust < FOX_TRUST) {
                    return Some(Goal {
                        at: on(f.n),
                        arrive: Fox::preferred_distance(f.trust) + 0.35,
                        still: true,
                        hop: false,
                    });
                }
                self.plane_parts.iter().filter(|q| !q.found).map(|q| on(q.n)).min_by(dist).map(walk)
            }
        }
    }

    /// Starts the planet over for a journey and puts back its saved score:
    /// sprouts pulled, sunsets watched or the fox's trust. Nothing from the
    /// journey or autoplay before carries over: sprouts are as they first
    /// grew and the fox is back in its place.
    /// `swept` lists the active volcanoes already swept, by their order
    /// among the active ones; `None` (saves before it was kept) counts them
    /// from the first.
    pub fn restore(&mut self, parts: [usize; 3], marks: Option<u8>) {
        let mut parts = parts;
        let active = self.volcanoes.iter().filter(|v| v.active).count();
        let swept = mark_swept(active, parts[1], marks);
        let found = mark_swept(self.plane_parts.len(), parts[1], marks);
        let count = |m: &[bool]| m.iter().filter(|m| **m).count();
        match self.kind {
            Kind::B612 => parts[1] = count(&swept),
            Kind::Desert => parts[1] = count(&found),
            Kind::Lamp => {}
        }
        for (q, f) in self.plane_parts.iter_mut().zip(found) {
            q.found = f;
            q.shown = 0.0;
        }
        self.moths.clear();
        self.moth_t = 0.0;
        self.devils.clear();
        self.storm = 0.0;
        self.storm_t = 0.0;
        if let Some(l) = &mut self.lamp {
            l.out = 0.0;
        }
        for (have, (saved, need)) in self.parts.iter_mut().zip(parts.into_iter().zip(self.needs)) {
            *have = saved.min(need);
        }
        self.score = self.parts.iter().sum();
        self.pulled = 0;
        self.sprouts.truncate(self.first_sprouts);
        for (v, swept) in self.volcanoes.iter_mut().filter(|v| v.active).zip(swept) {
            v.swept = swept;
            v.sweep = 0.0;
            v.pressure = 0.0;
        }
        for c in &mut self.caterpillars {
            c.reset();
        }
        self.cinders.clear();
        self.dust.reset();
        self.bump_cool = 0.0;
        self.pop = 0.0;
        // Not "the sun was up": loading at night would count a sunset.
        self.last_e = UNSEEN;
        for sp in &mut self.sprouts {
            sp.grow = sp.grow0;
            sp.pulled = None;
            sp.seed = 0.0;
        }
        if let Some(rose) = &mut self.rose {
            rose.near = false;
        }
        if self.fox.is_some() {
            let mut fox = Fox::new(self.spawn);
            fox.trust = self.parts[0];
            self.fox = Some(fox);
        }
    }

    /// The aeroplane parts found so far, bit `i` for the `i`th, for the save.
    pub fn found(&self) -> u8 {
        self.plane_parts.iter().enumerate().filter(|(_, q)| q.found).fold(0, |m, (i, _)| m | 1 << i)
    }

    /// Moves the sandstorm's cycle to just after it rises, for captures.
    pub fn skip_to_storm(&mut self) {
        self.storm_t = STORM_CALM + 4.0;
    }

    /// Snuffer moths flying at the lamp.
    pub fn moths_coming(&self) -> usize {
        self.moths.iter().filter(|m| m.coming()).count()
    }

    /// The push of the sandstorm's wind at `n`, as a share of walking pace.
    pub fn storm_push(&self, n: V3) -> V3 {
        let w = self.pal.wind.flatten(n);
        if w.len() < 1e-4 {
            V3::ZERO
        } else {
            w.norm() * (0.15 * self.storm)
        }
    }

    /// The active volcanoes swept so far, bit `i` for the `i`th, for the save.
    pub fn swept(&self) -> u8 {
        self.volcanoes.iter().filter(|v| v.active).enumerate().filter(|(_, v)| v.swept).fold(0, |m, (i, _)| m | 1 << i)
    }

    /// Whether a crawling caterpillar is within `r` of `feet`: then A jumps
    /// to land on it rather than talking to the rose beside it.
    pub fn caterpillar_near(&self, feet: V3, r: f32) -> bool {
        let kind = self.kind;
        self.caterpillars.iter().filter(|c| c.crawling()).any(|c| (c.n * ground_r(kind, c.n) - feet).len() < r)
    }

    /// Caterpillars out and crawling.
    pub fn crawling(&self) -> usize {
        self.caterpillars.iter().filter(|c| c.crawling()).count()
    }

    /// Sprouts on the planet, pulled or not.
    pub fn sprout_count(&self) -> usize {
        self.sprouts.len()
    }

    /// Sprouts pulled up and not yet grown back.
    pub fn sprouts_out(&self) -> usize {
        self.sprouts.iter().filter(|s| s.pulled.is_some()).count()
    }

    /// Whether the planet's task asks for nothing more: its score is full.
    /// Talking to its character then completes it.
    pub fn ready(&self) -> bool {
        self.total > 0 && self.score >= self.total
    }

    /// The lamplighter, for drawing him from a model.
    pub fn keeper_actor(&self) -> Option<Actor> {
        self.keeper.as_ref().map(|k| Actor {
            frame: Frame::new(k.pos, k.n, k.face),
            speed: 0.0,
            busy: k.anim.is_some(),
        })
    }

    /// Where the lamplighter's model holds his pole this frame.
    pub fn set_keeper_hand(&mut self, hand: Option<V3>) {
        if let Some(k) = &mut self.keeper {
            k.hand = hand;
        }
    }

    /// The fox, for drawing it from a model.
    pub fn fox_actor(&self) -> Option<Actor> {
        self.fox.as_ref().map(|f| {
            let at = f.n * ground_r(self.kind, f.n);
            Actor { frame: Frame::new(at, f.n, f.face), speed: f.speed, busy: f.sit > 0.6 }
        })
    }

    /// Where the planet's character stands: the rose, the lamplighter or
    /// the fox.
    pub fn speaker(&self) -> Option<V3> {
        match self.kind {
            Kind::B612 => self.rose.as_ref().map(|r| r.pos),
            Kind::Lamp => self.keeper.as_ref().map(|k| k.pos),
            Kind::Desert => self.fox.as_ref().map(|f| f.n * ground_r(self.kind, f.n)),
        }
    }

    /// Adds `k` to part `i` of the task, up to what it needs; whether that
    /// counted.
    fn add_part(&mut self, i: usize, k: usize) -> bool {
        let was = self.parts[i];
        self.parts[i] = (was + k).min(self.needs[i]);
        self.score = self.parts.iter().sum();
        let up = self.parts[i] > was;
        if up {
            self.pop = 1.0;
        }
        up
    }

    /// A trouble at `from` knocks the prince back, once in a while.
    fn knock(&mut self, from: V3) {
        if self.bump_cool <= 0.0 {
            self.bump = Some(from);
            self.bump_cool = 1.2;
        }
    }

    /// Stardust the prince spilled at his feet, `n` motes.
    pub fn spill(&mut self, feet: V3, n: usize) {
        self.dust.scatter(feet.norm(), n, self.kind.radius());
    }

    /// B612's troubles: volcanoes building up and being swept, the cinders
    /// they throw, sprouts spreading, and caterpillars making for the rose.
    /// Returns true when a part of the task moved on.
    fn update_troubles(&mut self, dt: f32, p: &PState, fx: &mut Fx, snd: &mut Sounds) -> bool {
        let kind = self.kind;
        let rad = kind.radius();
        let ground = move |n: V3| ground_r(kind, n);
        let mut scored = false;
        let mut swept_now = Vec::new();
        let mut erupt = Vec::new();
        for (i, v) in self.volcanoes.iter_mut().enumerate().filter(|(_, v)| v.active) {
            let base = v.n * ground(v.n);
            if (p.feet - base).len() < 1.15 {
                v.sweep += dt;
                if self.rng.f() < dt * 10.0 {
                    let side = any_perp(v.n).rotate(v.n, self.rng.range(0.0, TAU));
                    fx.burst(
                        base + side * 0.5 + v.n * 0.05,
                        v.n,
                        Burst {
                            n: 3,
                            speed: (0.2, 0.5),
                            size: (0.03, 0.06),
                            life: (0.4, 0.8),
                            cols: &[rgb(0xB8A8A0), rgb(0x8E8896)],
                            glow: false,
                            grav: 1.0,
                            drag: 1.5,
                            aim: 0.5,
                        },
                    );
                }
                if v.sweep >= SWEEP {
                    v.sweep = 0.0;
                    v.pressure = -2.0;
                    swept_now.push(i);
                }
            } else {
                v.sweep = (v.sweep - dt * 0.5).max(0.0);
            }
            v.pressure += dt / 16.0;
            if v.pressure >= 1.0 {
                v.pressure = 0.0;
                erupt.push(i);
            }
        }
        for i in swept_now {
            let v = &mut self.volcanoes[i];
            let (n, first) = (v.n, !v.swept);
            v.swept = true;
            if first {
                scored |= self.add_part(1, 1);
            }
            self.handled += 1;
            self.dust.scatter(n, 3, rad);
            fx.ring(n * ground(n), n, 1.1, 0.8, rgb(0xFFE9A8));
            snd.play(Sfx::Chime(2), 1.0, 0.4);
        }
        for i in erupt {
            let (top, n) = (self.volcanoes[i].top, self.volcanoes[i].n);
            let me = p.feet.norm();
            // One cinder is meant for a prince standing near.
            let near = (p.feet - n * ground(n)).len() < 5.0;
            for k in 0..5 {
                let (from, dist) = if k == 0 && near { (me, (0.0, 0.4)) } else { (n, (1.2, 4.0)) };
                let side = any_perp(from).rotate(from, self.rng.range(0.0, TAU));
                let to = from.rotate(side, self.rng.range(dist.0, dist.1) / rad).norm();
                self.cinders.push(Cinder { from: top, to, t: 0.0, dur: self.rng.range(1.5, 2.1) });
            }
            fx.burst(
                top,
                n,
                Burst {
                    n: 24,
                    speed: (1.0, 2.4),
                    size: (0.03, 0.07),
                    life: (0.5, 1.1),
                    cols: &[rgb(0xFF8A3D), rgb(0xFFD27A)],
                    glow: true,
                    grav: 2.5,
                    drag: 0.6,
                    aim: 0.7,
                },
            );
            if near {
                fx.shake = fx.shake.max(0.25);
            }
            snd.play(Sfx::Whoosh, 0.5, 0.5);
        }
        let mut landed = Vec::new();
        for c in &mut self.cinders {
            c.t += dt;
            if c.landed() {
                landed.push(c.to);
            }
        }
        self.cinders.retain(|c| !c.landed());
        for to in landed {
            let at = to * ground(to);
            fx.burst(
                at,
                to,
                Burst {
                    n: 14,
                    speed: (0.4, 1.4),
                    size: (0.03, 0.06),
                    life: (0.3, 0.8),
                    cols: &[rgb(0xFF7A2E), rgb(0xFFD27A), rgb(0x5A3A30)],
                    glow: true,
                    grav: 3.0,
                    drag: 1.0,
                    aim: 0.6,
                },
            );
            fx.ring(at, to, 0.5, 0.5, rgb(0xFF9A50));
            snd.play(Sfx::Pop, 0.6, 0.35);
            if (p.feet - at).len() < 0.6 {
                self.knock(at);
            }
        }
        // Sprouts left to grow seed others nearby.
        let mut seeds = Vec::new();
        for s in self.sprouts.iter_mut().filter(|s| s.pulled.is_none() && s.grow >= 1.0) {
            s.seed += dt;
            if s.seed > 20.0 {
                s.seed = 0.0;
                seeds.push(s.n);
            }
        }
        for n in seeds {
            if self.sprouts.len() >= MAX_SPROUTS {
                break;
            }
            let side = any_perp(n).rotate(n, self.rng.range(0.0, TAU));
            let d = n.rotate(side, self.rng.range(0.7, 1.6) / rad).norm();
            let clear = self.sprouts.iter().all(|o| (o.n - d).len() * rad > 0.45)
                && self.rose.as_ref().is_none_or(|r| (r.n - d).len() * rad > 0.9)
                && path_dist(d) > 0.35;
            if clear {
                let pos = d * ground(d);
                let phase = self.rng.f() * TAU;
                self.sprouts.push(Sprout { n: d, pos, grow: 0.0, grow0: 0.0, pulled: None, phase, seed: 0.0 });
                fx.burst(
                    pos,
                    d,
                    Burst {
                        n: 8,
                        speed: (0.2, 0.5),
                        size: (0.02, 0.04),
                        life: (0.4, 0.8),
                        cols: &[rgb(0x7CC35A), rgb(0x8A6A4A)],
                        glow: false,
                        grav: 2.0,
                        drag: 1.0,
                        aim: 0.6,
                    },
                );
            }
        }
        // Caterpillars: landed on, they turn into butterflies; walked into,
        // they knock the prince back.
        let goal = self.rose.as_ref().map_or(self.spawn, |r| r.n);
        let mut stomped = Vec::new();
        let mut bumped = None;
        for (i, c) in self.caterpillars.iter_mut().enumerate() {
            c.update(dt, goal, rad);
            if !c.crawling() {
                continue;
            }
            let at = c.n * ground(c.n);
            let d = (p.feet - at).len();
            if p.landed && d < 0.5 {
                c.curl = Some(0.0);
                stomped.push(i);
            } else if p.grounded && d < 0.2 {
                bumped = Some(at);
            }
        }
        if let Some(at) = bumped {
            self.knock(at);
        }
        for i in stomped {
            let n = self.caterpillars[i].n;
            let at = n * ground(n);
            scored |= self.add_part(2, 1);
            self.handled += 1;
            self.dust.scatter(n, 3, rad);
            let col = [rgb(0xFFF3A8), rgb(0xFFB0C8), rgb(0xA8D8FF)][i % 3];
            if self.butterflies.len() < 60 {
                let phase = self.rng.f() * TAU;
                self.butterflies.push((n, phase, col));
            }
            fx.burst(
                at + n * 0.1,
                n,
                Burst {
                    n: 18,
                    speed: (0.3, 1.0),
                    size: (0.025, 0.05),
                    life: (0.6, 1.2),
                    cols: &[col, rgb(0xFFFFFF)],
                    glow: true,
                    grav: -0.3,
                    drag: 1.5,
                    aim: 0.5,
                },
            );
            snd.play(Sfx::Chime(5), 1.2, 0.4);
        }
        scored
    }

    /// The lamplighter's night watch: snuffer moths come out of the dark
    /// for the lamp. Landing near one, or jumping into it, shoos it off as a
    /// firefly; one that reaches the lamp puts it out for a while; walking
    /// into one knocks the prince back. Returns true when a moth was shooed.
    fn update_moths(&mut self, dt: f32, sun: V3, p: &PState, fx: &mut Fx, snd: &mut Sounds) -> bool {
        let Some(ln) = self.lamp.as_ref().map(|l| l.n) else { return false };
        let kind = self.kind;
        let rad = kind.radius();
        let ground = move |n: V3| ground_r(kind, n);
        let night = ln.dot(sun) < -0.1;
        let top = 0.93;
        self.moth_t -= dt;
        if night && self.moths_coming() < 2 && self.moth_t <= 0.0 {
            self.moth_t = self.rng.range(1.5, 2.6);
            let side = any_perp(ln).rotate(ln, self.rng.range(0.0, TAU));
            let n = ln.rotate(side, self.rng.range(2.6, 3.6) / rad).norm();
            let phase = self.rng.f() * TAU;
            self.moths.push(Moth::new(n, ln, phase));
        }
        let (mut snuffed, mut shooed, mut bump) = (false, Vec::new(), None);
        let up = p.feet.norm();
        for m in &mut self.moths {
            if m.update(dt, ln, top, rad) {
                m.flee = Some(0.0);
                snuffed = true;
                continue;
            }
            if !m.coming() {
                continue;
            }
            if !night {
                // Off into the daylight.
                m.flee = Some(0.0);
                continue;
            }
            let at = m.pos(&ground);
            let flat = (at - p.feet).flatten(up).len();
            let near_jump = !p.grounded && (at - (p.feet + up * 0.3)).len() < 0.42;
            if (p.landed && flat < 0.65) || near_jump {
                m.flee = Some(0.0);
                shooed.push(at);
            } else if p.grounded && flat < 0.18 && m.alt < 0.75 {
                bump = Some(at);
            }
        }
        self.moths.retain(|m| !m.gone());
        let mut scored = false;
        for at in shooed {
            scored |= self.add_part(1, 1);
            self.handled += 1;
            self.dust.scatter(at.norm(), 2, rad);
            fx.burst(
                at,
                at.norm(),
                Burst {
                    n: 14,
                    speed: (0.2, 0.7),
                    size: (0.02, 0.045),
                    life: (0.8, 1.6),
                    cols: &[rgb(0xD8FF8A), rgb(0xFFF4B0)],
                    glow: true,
                    grav: -0.4,
                    drag: 1.4,
                    aim: 0.4,
                },
            );
            snd.play(Sfx::Chime(5), 1.3, 0.35);
        }
        if snuffed {
            if let Some(l) = self.lamp.as_mut().filter(|l| l.lit) {
                l.lit = false;
                l.out = 1.6;
                fx.burst(
                    l.head,
                    l.n,
                    Burst {
                        n: 12,
                        speed: (0.2, 0.5),
                        size: (0.04, 0.08),
                        life: (0.8, 1.4),
                        cols: &[rgb(0x9A94A8), rgb(0xC9C4D6)],
                        glow: false,
                        grav: -0.3,
                        drag: 1.2,
                        aim: 0.7,
                    },
                );
                snd.play(Sfx::Pop, 0.5, 0.4);
            }
        }
        if let Some(at) = bump {
            self.knock(at);
        }
        scored
    }

    /// The desert: the sandstorm comes and goes, raising dust devils that
    /// knock the prince back; the fox, once tamed, digs up the buried
    /// aeroplane parts; walking over a part picks it up.
    fn update_desert(&mut self, dt: f32, p: &PState, fx: &mut Fx, snd: &mut Sounds) -> bool {
        let kind = self.kind;
        let rad = kind.radius();
        let on = move |n: V3| n * ground_r(kind, n);
        let mut scored = false;
        self.storm_t += dt;
        let c = self.storm_t % STORM_CYCLE;
        self.storm = smooth((c - STORM_CALM) / 3.0) * (1.0 - smooth((c - STORM_CALM - 16.0) / 3.0));
        let up = p.feet.norm();
        let wind = self.pal.wind.flatten(up);
        let wind = if wind.len() < 1e-4 { any_perp(up) } else { wind.norm() };
        for _ in 0..3 {
            if self.rng.f() < dt * 40.0 * self.storm {
                let side = any_perp(up).rotate(up, self.rng.range(0.0, TAU));
                let at = p.feet + side * self.rng.range(0.3, 3.0) + up * self.rng.range(0.05, 0.9) - wind * 1.5;
                let v = wind * self.rng.range(2.0, 3.5) + up * self.rng.range(-0.1, 0.2);
                let mut g =
                    Particle::new(at, v, self.rng.range(0.8, 1.4), self.rng.range(0.015, 0.03), rgb(0xE2BC86), false);
                g.drag = 0.2;
                g.grav = 0.0;
                fx.spawn(g);
            }
        }
        if self.storm > 0.6 && self.devils.len() < 2 && self.rng.f() < dt * 0.8 {
            let side = any_perp(up).rotate(up, self.rng.range(0.0, TAU));
            let n = up.rotate(side, self.rng.range(3.0, 4.5) / rad).norm();
            let face = (up - n).flatten(n).norm();
            let life = self.rng.range(7.0, 10.0);
            self.devils.push(Devil::new(n, face, life));
        }
        for d in &mut self.devils {
            d.update(dt, up, rad);
        }
        self.devils.retain(|d| d.life > 0.0);
        let hit = self.devils.iter().find(|d| d.strength() > 0.5 && (on(d.n) - p.feet).len() < 0.35).map(|d| on(d.n));
        if let Some(at) = hit {
            self.knock(at);
        }
        // Once tamed, the fox shows where the sand hides a part.
        let tamed = self.fox.as_ref().is_some_and(|f| f.trust >= FOX_TRUST);
        let lead = if tamed {
            self.plane_parts
                .iter()
                .filter(|q| q.buried && q.shown <= 0.0 && !q.found && (on(q.n) - p.feet).len() < 7.0)
                .map(|q| on(q.n))
                .min_by(|a, b| (*a - p.feet).len().total_cmp(&(*b - p.feet).len()))
        } else {
            None
        };
        if let Some(fox) = &mut self.fox {
            scored |= fox.update(dt, p, lead, fx, snd);
        }
        let fox_at = self.fox.as_ref().map(|f| on(f.n));
        for q in &mut self.plane_parts {
            if q.shown > 0.0 {
                q.shown += dt;
            } else if tamed && q.buried && !q.found && fox_at.is_some_and(|f| (on(q.n) - f).len() < 0.45) {
                q.shown = 0.001;
                fx.burst(
                    on(q.n),
                    q.n,
                    Burst {
                        n: 18,
                        speed: (0.4, 1.0),
                        size: (0.03, 0.06),
                        life: (0.5, 1.0),
                        cols: &[rgb(0xE2BC86), rgb(0xC99A60)],
                        glow: false,
                        grav: 3.0,
                        drag: 1.0,
                        aim: 0.7,
                    },
                );
                snd.play(Sfx::Chime(3), 1.2, 0.4);
            }
        }
        let mut got = 0;
        for q in self.plane_parts.iter_mut().filter(|q| q.visible()) {
            if (on(q.n) - p.feet).len() < 0.45 {
                q.found = true;
                got += 1;
                fx.ring(on(q.n), q.n, 0.7, 0.7, rgb(0xFFE9A8));
                snd.play(Sfx::Chime(6), 1.0, 0.45);
            }
        }
        if got > 0 {
            scored |= self.add_part(1, got);
            self.handled += got as u32;
            self.dust.scatter(up, 2 * got, rad);
        }
        let trust = self.fox.as_ref().map_or(0, |f| f.trust);
        if trust > self.parts[0] {
            scored |= self.add_part(0, trust - self.parts[0]);
        }
        scored
    }

    /// Advances the planet. Returns true when the prince achieved something.
    pub fn update(&mut self, dt: f32, t: f32, p: &PState, fx: &mut Fx, snd: &mut Sounds) -> bool {
        self.pop = (self.pop - dt * 2.5).max(0.0);
        self.gathered = 0;
        self.handled = 0;
        self.bump = None;
        self.bump_cool -= dt;
        let mut scored = false;
        let kind = self.kind;
        self.gathered = self.dust.update(dt, t, p.feet, &|n| ground_r(kind, n), fx, snd);
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
                self.pulled = pulled;
                if pulled > 0 {
                    scored |= self.add_part(0, pulled);
                    self.dust.scatter(p.feet.norm(), pulled, self.kind.radius());
                }
                scored |= self.update_troubles(dt, p, fx, snd);
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
                    lamp.out = (lamp.out - dt).max(0.0);
                    match &mut k.anim {
                        None if want != lamp.lit && lamp.out <= 0.0 => k.anim = Some((0.0, want)),
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
                if sets(self.last_e, e) {
                    scored |= self.add_part(0, 1);
                    snd.play(Sfx::Chime(0), 0.8, 0.3);
                }
                self.last_e = e;
            }
            Kind::Desert => {
                scored |= self.update_desert(dt, p, fx, snd);
            }
        }
        if self.kind == Kind::Lamp {
            scored |= self.update_moths(dt, sun, p, fx, snd);
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
    pub fn draw_sky(&self, gl: &Gl, s: &Sprites, cam: V3, t: f32, warp: f32, sky: &Sky, d: f32) {
        // The sky sits 26 units away, in front of the backdrop at 30: on a
        // 16-bit depth buffer anything much farther rounds to the far plane.
        let k = d / 70.0;
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
    fn queue_tiles(&self, gl: &Gl, cam: V3, fwd: V3, detail: f32) {
        let (close, near, group, mid) = (CLOSE * detail, NEAR * detail, GROUP * detail, MID * detail);
        let cells = self.tiles.iter().map(|t| (t.dir, t.ang, t.low));
        let horizon = Horizon::new(cam, self.floor, cells);
        let mut visible = Vec::with_capacity(self.tiles.len());
        // Preserve the existing tie order for equally distant cells.
        for tile in self.tiles.iter().rev() {
            let view = View { cam, fwd, floor: horizon.floor(tile.dir, tile.ang), radius: self.radius };
            if let Some(dist) = view.tile(tile.dir, tile.ang, tile.top) {
                visible.push((dist, tile));
            }
        }
        visible.sort_by(|a, b| a.0.total_cmp(&b.0));
        // A group whose cells in view are all some way off is drawn as one
        // mesh, where its nearest cell would have been.
        let mut near_cells = vec![0u8; self.groups.len()];
        let mut grouped = vec![0u8; self.groups.len()];
        for (dist, tile) in &visible {
            if *dist < group {
                near_cells[tile.group] += 1;
            } else {
                grouped[tile.group] += 1;
            }
        }
        let mut drawn = vec![false; self.groups.len()];
        for (dist, tile) in visible {
            let cells = &self.groups[tile.group];
            let (mesh, lod) = if dist < close {
                (tile.close, "close")
            } else if dist < near {
                (tile.near, "near")
            } else if near_cells[tile.group] == 0 && grouped[tile.group] > 1 {
                if std::mem::replace(&mut drawn[tile.group], true) {
                    continue;
                }
                if dist < mid {
                    (cells.mid, "mid")
                } else {
                    (cells.far, "far")
                }
            } else if dist < mid {
                (tile.mid, "mid")
            } else {
                (tile.far, "far")
            };
            gl.surface(mesh, lod);
        }
    }

    /// The solid parts of the planet, queued for `Gl::flush`. `detail`
    /// stretches the distances at which cells lose detail.
    pub fn draw(&self, gl: &Gl, kit: &Kit, t: f32, cam: V3, fwd: V3, detail: f32) {
        if !crate::prof::skip("world") {
            self.queue_tiles(gl, cam, fwd, detail);
        }
        let wind = self.pal.wind;
        let ri = Ink::Radial;
        let kind = self.kind;
        let ground = move |n: V3| ground_r(kind, n);
        let troubles = !crate::prof::skip("troubles");
        // Too small to make out beyond a few steps.
        let near = |n: V3| troubles && (n * ground(n) - cam).len() < 8.0;
        for c in self.caterpillars.iter().filter(|c| near(c.n)) {
            c.draw(gl, kit, &ground);
        }
        for m in self.moths.iter().filter(|m| near(m.n)) {
            m.draw(gl, kit, &ground);
        }
        for q in self.plane_parts.iter().filter(|q| near(q.n)) {
            q.draw(gl, kit, &ground, t);
        }
        for c in self.cinders.iter().filter(|_| troubles) {
            let at = c.pos(&ground);
            gl.part(kit.ball, place(at, c.to, any_perp(c.to), v3(0.05, 0.05, 0.05)), rgb(0x6A2A18), 0.6, Ink::None);
        }
        for v in &self.volcanoes {
            if v.active {
                let x = any_perp(v.n);
                let glow = 1.0 + 0.4 * (t * 3.0 + v.puff).sin() + v.pressure.max(0.0) * 1.5;
                gl.part(
                    kit.ball,
                    place(v.top - v.n * 0.004, v.n, x, v3(0.07, 0.008, 0.07)),
                    rgb(0xFF8A3D),
                    glow,
                    Ink::None,
                );
            }
        }
        // Sprouts that spread are troubles too.
        let shown = if troubles { self.sprouts.len() } else { self.first_sprouts };
        for s in &self.sprouts[..shown.min(self.sprouts.len())] {
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
            if self.models {
                World::draw_pole(gl, kit, k);
            } else {
                self.draw_keeper(gl, kit, k, t);
            }
        }
        if let Some(f) = self.fox.as_ref().filter(|_| !self.models) {
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
        let hand = k.hand.unwrap_or_else(|| rest.lerp(up, k.raise));
        let pole_dir = f.dir(v3(0.05, 1.0, 0.12).lerp(v3(-0.05, 1.0, 0.55), k.raise)).norm();
        (f, hand, hand + pole_dir * 0.62)
    }

    fn draw_keeper(&self, gl: &Gl, kit: &Kit, k: &Keeper, t: f32) {
        let (f, hand, _) = World::keeper_pose(k);
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
        World::draw_pole(gl, kit, k);
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

    /// The lamplighter's pole, with its little flame at the tip.
    fn draw_pole(gl: &Gl, kit: &Kit, k: &Keeper) {
        let (_, hand, tip) = World::keeper_pose(k);
        let pd = tip - hand;
        gl.part(
            kit.tube,
            place(hand - pd * 0.15, pd, k.face, v3(0.008, pd.len() * 1.15, 0.008)),
            rgb(0x8A5A3B),
            0.0,
            Ink::Radial,
        );
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
        let kind = self.kind;
        if !self.cinders.is_empty() {
            Sprites::begin(gl.sprite);
            trouble::draw_cinder_shadows(&self.cinders, &move |n| ground_r(kind, n));
            Sprites::end();
        }
        if !self.devils.is_empty() {
            Sprites::begin(gl.sprite);
            for d in &self.devils {
                d.draw(cam, &move |n| ground_r(kind, n));
            }
            Sprites::end();
        }
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
            let k = (0.8 + 0.2 * (t * 3.0 + v.puff).sin()) * (1.0 + v.pressure.max(0.0));
            s.billboard(v.top + v.n * 0.03, 0.22 * k, rgba(rgb(0xFF8A3D), 0.45));
        }
        let kind = self.kind;
        let ground = move |n: V3| ground_r(kind, n);
        self.dust.draw_glow(s, t, &ground);
        trouble::draw_cinder_glow(&self.cinders, s, t, &ground);
        for q in &self.plane_parts {
            q.draw_glint(s, &ground, t);
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

/// The lowest ground round the camera, by direction: what can hide things
/// from it.
///
/// A line of sight that dips below a sphere of radius `R` enters it within
/// `acos(R / |cam|)` of the camera, over the arc towards its target. So for
/// each of `SECTORS` directions round the camera this keeps the lowest ground
/// of the cells within that reach and that direction: a sphere that size
/// hides no more than the ground does there.
struct Horizon {
    cam_dir: V3,
    x: V3,
    z: V3,
    floors: [f32; SECTORS],
}

const SECTORS: usize = 16;

impl Horizon {
    /// `floor` is the lowest ground on the planet; `cells` give each cell's
    /// centre, angular size and lowest ground.
    fn new(cam: V3, floor: f32, cells: impl Iterator<Item = (V3, f32, f32)>) -> Horizon {
        let dist = cam.len();
        let cam_dir = cam.norm();
        let x = any_perp(cam_dir);
        let mut h = Horizon { cam_dir, x, z: cam_dir.cross(x), floors: [dist; SECTORS] };
        let reach = (floor / dist.max(floor)).acos() + 0.02;
        for (dir, ang, low) in cells {
            if h.theta(dir) - ang <= reach {
                for k in h.sectors(dir, ang) {
                    h.floors[k] = h.floors[k].min(low);
                }
            }
        }
        h
    }

    fn theta(&self, dir: V3) -> f32 {
        dir.dot(self.cam_dir).clamp(-1.0, 1.0).acos()
    }

    /// The sectors a cell of angular size `ang` round `dir` reaches into.
    fn sectors(&self, dir: V3, ang: f32) -> impl Iterator<Item = usize> {
        let theta = self.theta(dir);
        let width = TAU / SECTORS as f32;
        let (first, count) = if theta <= ang + 0.01 {
            (0, SECTORS)
        } else {
            let az = dir.dot(self.z).atan2(dir.dot(self.x));
            let half = (ang.sin() / theta.sin()).min(1.0).asin() + 0.01;
            let lo = ((az - half) / width).floor() as i32;
            let hi = ((az + half) / width).floor() as i32;
            (lo, ((hi - lo + 1) as usize).min(SECTORS))
        };
        (0..count).map(move |i| (first + i as i32).rem_euclid(SECTORS as i32) as usize)
    }

    /// The lowest ground between the camera and a cell.
    fn floor(&self, dir: V3, ang: f32) -> f32 {
        self.sectors(dir, ang).map(|k| self.floors[k]).fold(f32::MAX, f32::min)
    }
}

/// Where the camera is, for choosing the cells to draw.
struct View {
    cam: V3,
    fwd: V3,
    /// The lowest ground on the planet: nothing hides the view to a cell
    /// more than a smooth sphere this size would.
    floor: f32,
    radius: f32,
}

impl View {
    /// How far away a cell is, or None when it is below the horizon, behind
    /// the camera or too far to draw. `dir` is its centre, `ang` the angle
    /// to its farthest vertex and `top` its highest point from the centre.
    fn tile(&self, dir: V3, ang: f32, top: f32) -> Option<f32> {
        const VIEW_DISTANCE: f32 = 60.0;
        let (r, floor) = (self.radius, self.floor);
        let cam_dist = self.cam.len();
        let cam_dir = self.cam.norm();
        let cam_horizon = (floor / cam_dist.max(floor)).acos();
        // A cell shows above the horizon as far as its highest point reaches.
        let top_horizon = (floor / top.max(floor + 0.3)).acos();
        let theta = dir.dot(cam_dir).clamp(-1.0, 1.0).acos();
        if theta - ang > cam_horizon + top_horizon + 0.05 {
            return None;
        }
        let extent = r * ang;
        let bound = extent + (top - r).max(r - floor).max(0.3);
        let delta = dir * r - self.cam;
        let dist = delta.len();
        (dist - bound < VIEW_DISTANCE && delta.dot(self.fwd) > -bound).then_some((dist - extent).max(0.0))
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
    #[test]
    fn the_volcano_swept_is_the_one_restored() {
        assert_eq!(mark_swept(2, 1, Some(0b10)), [false, true], "the second, swept first, stays swept");
        assert_eq!(mark_swept(2, 2, Some(0)), [false, false], "the saved volcanoes win over the count");
        assert_eq!(mark_swept(2, 1, None), [true, false], "an older save counts from the first");
    }

    use super::*;

    #[test]
    fn planets_are_between_15_and_25_units_across_with_the_lamplighters_the_smallest() {
        for kind in KINDS {
            let r = kind.radius();
            assert!((15.0..=25.0).contains(&r), "{kind:?} {r}");
            let mut rng = Rng::new(5);
            for _ in 0..2000 {
                let h = ground_r(kind, rng.dir());
                assert!((h - r).abs() < 2.5, "{kind:?} ground at {h}");
            }
        }
        assert!(KINDS.iter().all(|k| Kind::Lamp.radius() <= k.radius()));
        assert!(KINDS.iter().all(|k| Kind::Desert.radius() >= k.radius()));
    }

    #[test]
    fn a_camera_low_in_a_valley_still_sees_the_ground_ahead() {
        // B612's ground dips about 1.5 units below its spawn radius.
        let (radius, floor) = (18.0, 16.5);
        let n = V3::Y;
        let ahead = v3(0.0, 0.0, 1.0);
        let view = View { cam: n * 16.9, fwd: ahead, floor, radius };
        let dir = n.rotate(v3(1.0, 0.0, 0.0), 0.35);
        assert!(view.tile(dir, 0.05, 17.1).is_some());
        // The far side of the planet stays hidden.
        assert!(view.tile(-n, 0.05, 19.0).is_none());
        // Behind the camera, beyond its own cell, nothing is drawn.
        assert!(view.tile(n.rotate(v3(1.0, 0.0, 0.0), -0.5), 0.05, 18.0).is_none());
    }

    #[test]
    fn only_valleys_near_the_camera_and_towards_a_cell_lower_its_horizon() {
        let n = V3::Y;
        let toward = |f: f32, s: f32| n.rotate(v3(1.0, 0.0, 0.0), f).rotate(v3(0.0, 0.0, 1.0), s).norm();
        let cam = n * 18.5;
        let ahead = toward(0.6, 0.0);
        let cells =
            |valley: V3| [(n, 0.03, 17.8), (toward(0.2, 0.0), 0.03, 17.6), (valley, 0.03, 16.6), (-n, 0.03, 16.5)];
        // A valley behind the camera or on the far side does not matter.
        let behind = Horizon::new(cam, 16.5, cells(toward(-0.3, 0.0)).into_iter());
        assert_eq!(behind.floor(ahead, 0.03), 17.6);
        // One near the camera, on the way, does.
        let on_the_way = Horizon::new(cam, 16.5, cells(toward(0.3, 0.0)).into_iter());
        assert_eq!(on_the_way.floor(ahead, 0.03), 16.6);
        // The cell under the camera counts in every direction.
        let under = Horizon::new(cam, 16.5, [(n, 0.03, 16.9)].into_iter());
        assert_eq!(under.floor(toward(0.0, 0.6), 0.03), 16.9);
    }

    #[test]
    fn the_lamplighters_day_still_lasts_14_seconds() {
        assert_eq!(Kind::Lamp.day(), 14.0);
    }

    #[test]
    fn arriving_or_loading_at_night_is_not_a_sunset() {
        // The first frame after arriving or loading only notes the sun.
        assert!(!sets(UNSEEN, -0.6));
        assert!(!sets(UNSEEN, 0.4));
        assert!(sets(0.02, -0.01));
        assert!(!sets(-0.2, -0.3), "still night");
        assert!(!sets(0.3, 0.2), "still day");
    }

    #[test]
    fn the_fox_starts_a_walk_away_from_the_spawn() {
        let spawn = V3::Y;
        let fox = Fox::new(spawn);
        let r = Kind::Desert.radius();
        let d = (fox.n * r - spawn * r).len();
        assert!((3.0..5.0).contains(&d), "{d}");
    }

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
        let player = PState {
            feet: spawn * ground_r(Kind::Desert, spawn),
            face: any_perp(spawn),
            speed: 0.0,
            grounded: true,
            landed: false,
        };
        let mut fox = Fox::new(spawn);
        let mut fx = Fx::new();
        let mut snd = Sounds::new(false);
        for _ in 0..(20 * 60) {
            fox.update(1.0 / 60.0, &player, None, &mut fx, &mut snd);
        }
        assert_eq!(fox.trust, FOX_TRUST);
    }
}
