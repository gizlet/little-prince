//! Troubles and stardust.
//!
//! Troubles never hurt the prince: running into one knocks him back and
//! spills a little of his stardust, which he can gather up again. Stardust
//! motes float over every planet and drift back a while after they are
//! gathered; seeing to a trouble sets a few more free.
//!
//! B612's troubles live here: caterpillars crawling towards the rose, and
//! the cinders an unswept volcano throws. The sprouts and the volcanoes
//! themselves are in `world`.

use crate::fx::{Burst, Fx};
use crate::gl::{rgb, rgba, Gl, Ink, Sprites};
use crate::math::{any_perp, place, v3, Rng, V3};
use crate::mesh::Kit;
use crate::sound::{Sfx, Sounds};
use std::f32::consts::{PI, TAU};

/// Seconds before a gathered mote drifts back to its place.
const RETURN: f32 = 40.0;
/// Seconds a spilled or freed mote waits to be gathered.
const LINGER: f32 = 9.0;
/// Seconds a loose mote takes to fly out.
const FLY: f32 = 0.6;
/// How near the prince's feet a mote is gathered.
const REACH: f32 = 0.42;

struct Mote {
    /// Where it rests, as a direction from the planet's centre.
    n: V3,
    /// For a loose mote: where it flew out from and seconds since.
    from: Option<(V3, f32)>,
    /// Seconds until a gathered mote is back; 0 while it is there.
    wait: f32,
    phase: f32,
}

impl Mote {
    fn loose(&self) -> bool {
        self.from.is_some()
    }

    /// Its direction and height above the ground.
    fn at(&self, t: f32) -> (V3, f32) {
        let bob = 0.32 + 0.04 * (t * 2.0 + self.phase).sin();
        match self.from {
            Some((from, age)) if age < FLY => {
                let u = age / FLY;
                (from.lerp(self.n, u).norm(), bob + (u * PI).sin() * 0.45)
            }
            _ => (self.n, bob),
        }
    }
}

/// A planet's stardust.
pub struct Dust {
    motes: Vec<Mote>,
    rng: Rng,
}

impl Dust {
    /// Motes resting at the directions `spots`.
    pub fn new(spots: &[V3], seed: u64) -> Dust {
        let mut rng = Rng::new(seed);
        let motes = spots.iter().map(|&n| Mote { n, from: None, wait: 0.0, phase: rng.f() * TAU }).collect();
        Dust { motes, rng }
    }

    /// Every mote back in its place, the loose ones gone.
    pub fn reset(&mut self) {
        self.motes.retain(|m| !m.loose());
        for m in &mut self.motes {
            m.wait = 0.0;
        }
    }

    /// `count` loose motes flying out from the direction `n` to land a
    /// little way off, on a planet of `radius`.
    pub fn scatter(&mut self, n: V3, count: usize, radius: f32) {
        for _ in 0..count {
            let side = any_perp(n).rotate(n, self.rng.range(0.0, TAU));
            let to = n.rotate(side.cross(n).norm(), self.rng.range(0.7, 1.5) / radius).norm();
            let phase = self.rng.f() * TAU;
            self.motes.push(Mote { n: to, from: Some((n, 0.0)), wait: 0.0, phase });
        }
    }

    /// Moves the motes on and gathers those within reach of `feet`;
    /// returns how many were gathered.
    pub fn update(
        &mut self,
        dt: f32,
        t: f32,
        feet: V3,
        ground: &dyn Fn(V3) -> f32,
        fx: &mut Fx,
        snd: &mut Sounds,
    ) -> u32 {
        let mut got = 0;
        for m in &mut self.motes {
            if let Some((_, age)) = &mut m.from {
                *age += dt;
            }
            if m.wait > 0.0 {
                m.wait = (m.wait - dt).max(0.0);
                continue;
            }
            let flying = m.from.is_some_and(|(_, age)| age < FLY * 0.8);
            let (n, alt) = m.at(t);
            let p = n * (ground(n) + alt);
            if !flying && (p - feet).len() < REACH + alt * 0.5 {
                got += 1;
                m.wait = RETURN;
                fx.burst(
                    p,
                    n,
                    Burst {
                        n: 8,
                        speed: (0.3, 0.8),
                        size: (0.02, 0.04),
                        life: (0.4, 0.8),
                        cols: &[rgb(0xFFE79A), rgb(0xFFFFFF)],
                        glow: true,
                        grav: -0.3,
                        drag: 2.0,
                        aim: 0.4,
                    },
                );
                snd.play(Sfx::Chime(6), 1.4 + 0.1 * (got % 4) as f32, 0.25);
            }
        }
        // Loose motes are not put back: gathered or left too long, they go.
        self.motes.retain(|m| !(m.loose() && (m.wait > 0.0 || m.from.is_some_and(|(_, age)| age > LINGER))));
        got
    }

    /// The nearest mote there to gather within `within` of `from`, on the
    /// ground beneath it.
    pub fn nearest(&self, from: V3, within: f32, ground: &dyn Fn(V3) -> f32) -> Option<V3> {
        self.motes
            .iter()
            .filter(|m| m.wait <= 0.0 && m.from.is_none_or(|(_, age)| age > FLY))
            .map(|m| m.n * ground(m.n))
            .filter(|p| (*p - from).len() < within)
            .min_by(|a, b| (*a - from).len().total_cmp(&(*b - from).len()))
    }

    /// The motes there to gather (additive, inside `Sprites::begin`).
    pub fn draw_glow(&self, s: &Sprites, t: f32, ground: &dyn Fn(V3) -> f32) {
        for m in self.motes.iter().filter(|m| m.wait <= 0.0) {
            let (n, alt) = m.at(t);
            let p = n * (ground(n) + alt);
            // Loose motes blink as they are about to go.
            let fade = match m.from {
                Some((_, age)) if age > LINGER - 2.5 => 0.4 + 0.6 * (age * 12.0).sin().abs(),
                _ => 1.0,
            };
            let k = 0.85 + 0.15 * (t * 5.0 + m.phase).sin();
            s.billboard(p, 0.16 * k, rgba(rgb(0xFFD36E), 0.5 * fade));
            s.billboard(p, 0.05, rgba(rgb(0xFFFBE8), fade));
        }
    }
}

/// A caterpillar making for the rose.
pub struct Caterpillar {
    pub n: V3,
    pub face: V3,
    /// Where it comes out when it shows itself again.
    home: V3,
    phase: f32,
    /// Seconds until it shows itself; above 0 it is away.
    pub away: f32,
    /// Seconds into turning into a butterfly, once landed on.
    pub curl: Option<f32>,
}

/// How fast a caterpillar crawls, in world units a second.
const CRAWL: f32 = 0.12;
/// How long before a caterpillar that became a butterfly is followed by
/// another.
const NEXT: f32 = 45.0;

impl Caterpillar {
    pub fn new(home: V3, phase: f32) -> Caterpillar {
        Caterpillar { n: home, face: any_perp(home), home, phase, away: 0.0, curl: None }
    }

    /// Out and crawling: one that can be landed on or bumped into.
    pub fn crawling(&self) -> bool {
        self.away <= 0.0 && self.curl.is_none()
    }

    pub fn reset(&mut self) {
        self.n = self.home;
        self.away = 0.0;
        self.curl = None;
    }

    /// Crawls towards `goal` on a planet of `radius`, and stops beside it.
    /// Returns true when it has just become a butterfly.
    pub fn update(&mut self, dt: f32, goal: V3, radius: f32) -> bool {
        self.phase += dt * 6.0;
        if let Some(c) = &mut self.curl {
            *c += dt;
            if *c > 0.8 {
                self.curl = None;
                self.away = NEXT;
                self.n = self.home;
                return true;
            }
            return false;
        }
        if self.away > 0.0 {
            self.away -= dt;
            return false;
        }
        let to = (goal - self.n).flatten(self.n);
        if to.len() * radius > 0.32 {
            let want = to.norm();
            let a = self.face.cross(want).dot(self.n).atan2(self.face.dot(want));
            self.face = self.face.rotate(self.n, a.clamp(-1.5 * dt, 1.5 * dt) + (self.phase * 0.3).sin() * 0.4 * dt);
            // Inching: it moves on the stretch of each ripple.
            let step = CRAWL * dt * (0.4 + 0.6 * (self.phase).sin().max(0.0) * 1.6) / radius;
            let axis = self.n.cross(self.face).norm();
            self.n = self.n.rotate(axis, step).norm();
            self.face = self.face.rotate(axis, step).flatten(self.n).norm();
        }
        false
    }

    /// Its body: segments rippling along, curling up once landed on.
    pub fn draw(&self, gl: &Gl, kit: &Kit, ground: &dyn Fn(V3) -> f32) {
        if self.away > 0.0 {
            return;
        }
        let up = self.n;
        let base = up * ground(up);
        let fwd = self.face;
        let side = up.cross(fwd).norm();
        let curl = self.curl.map_or(0.0, |c| (c / 0.5).min(1.0));
        let shrink = 1.0 - self.curl.map_or(0.0, |c| ((c - 0.5) / 0.3).clamp(0.0, 1.0));
        for i in 0..6 {
            let k = i as f32;
            let wave = (self.phase - k * 0.9).sin();
            // Laid out in a line, or rolled into a ring.
            let line = -fwd * (k * 0.042) + up * (0.03 + 0.016 * wave.max(0.0));
            let a = k / 6.0 * TAU;
            let ring = (-fwd * (1.0 - a.cos()) + up * a.sin()) * 0.05 + up * 0.05;
            let p = base + line * (1.0 - curl) + ring * curl;
            let r = if i == 0 { 0.034 } else { 0.03 - k * 0.002 } * shrink;
            let col = if i == 0 {
                rgb(0x5E9C3A)
            } else if i % 2 == 1 {
                rgb(0x9BCB45)
            } else {
                rgb(0xC4DE55)
            };
            // Only the head is inked: the body reads well without, at half the cost.
            let ink = if i == 0 { Ink::Radial } else { Ink::None };
            gl.part(kit.ball, place(p, up, fwd, v3(r, r * 0.95, r)), col, 0.0, ink);
            if i == 0 && curl < 0.5 {
                for s in [-1.0f32, 1.0] {
                    let eye = p + fwd * (r * 0.8) + side * (s * r * 0.45) + up * (r * 0.3);
                    gl.part(kit.ball, place(eye, up, fwd, v3(0.007, 0.007, 0.007)), rgb(0x1E1A2A), 0.0, Ink::None);
                    let tip = (up * 1.0 + fwd * 0.5 + side * (s * 0.35)).norm();
                    let root = p + up * (r * 0.7) + side * (s * r * 0.3);
                    gl.part(kit.tube, place(root, tip, fwd, v3(0.003, 0.035, 0.003)), rgb(0x3E5A2A), 0.0, Ink::None);
                }
            }
        }
    }
}

/// A cinder from a volcano, in flight to where it lands.
pub struct Cinder {
    pub from: V3,
    /// Where it lands, as a direction.
    pub to: V3,
    pub t: f32,
    pub dur: f32,
}

impl Cinder {
    /// Where it is now, given the ground's height.
    pub fn pos(&self, ground: &dyn Fn(V3) -> f32) -> V3 {
        let u = (self.t / self.dur).min(1.0);
        let land = self.to * ground(self.to);
        let up = self.from.lerp(land, 0.5).norm();
        self.from.lerp(land, u) + up * ((u * PI).sin() * 1.8)
    }

    pub fn landed(&self) -> bool {
        self.t >= self.dur
    }
}

/// The cinders in flight and the warning shadows where they will land
/// (see-through, inside `gl::alpha` with the sprite texture bound).
pub fn draw_cinder_shadows(cinders: &[Cinder], ground: &dyn Fn(V3) -> f32) {
    for c in cinders {
        let u = (c.t / c.dur).min(1.0);
        let n = c.to;
        let p = n * (ground(n) + 0.02);
        let a = any_perp(n);
        let b = n.cross(a);
        let r = 0.2 + 0.25 * u;
        Sprites::quad(p, a * r, b * r, rgba(rgb(0x5A1E10), 0.25 + 0.45 * u));
    }
}

/// Glowing cinders in flight (additive, inside `Sprites::begin`).
pub fn draw_cinder_glow(cinders: &[Cinder], s: &Sprites, t: f32, ground: &dyn Fn(V3) -> f32) {
    for c in cinders {
        let p = c.pos(ground);
        let f = 0.85 + 0.15 * (t * 25.0 + c.dur * 7.0).sin();
        s.billboard(p, 0.22 * f, rgba(rgb(0xFF7A2E), 0.7));
        s.billboard(p, 0.07, rgba(rgb(0xFFE6B0), 1.0));
    }
}
