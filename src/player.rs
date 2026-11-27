//! The little prince, after the book: golden hair standing up in tufts, a
//! long green coat with a belt and gold epaulettes, boots, and a long yellow
//! scarf that streams in the wind.
//!
//! All of his motion is procedural: a walk cycle with bobbing and arm swing,
//! leaning into speed and turns, squash and stretch on jumps and landings,
//! hair on springs, blinking, idle glances, arms raised when he is happy or
//! carried by the birds, and the scarf as a verlet rope blown by the wind.

use crate::gl::{rgb, rgba, Gl, Ink, Lighting, Sprites};
use crate::math::{any_perp, approach, approach3, place, smooth, v3, Frame, V3};
use crate::mesh::Kit;
use raylib_sys as ffi;
use std::f32::consts::{PI, TAU};

pub const SPEED: f32 = 1.9;
const GRAVITY: f32 = 7.5;
const JUMP: f32 = 2.9;
const SCARF: usize = 12;
/// His size against the planets; every part below is drawn in this frame.
pub const S: f32 = 0.56;
const SEG: f32 = 0.05 * S;

pub enum Ev {
    Step(V3),
    Jump(V3),
    Land(V3, f32),
}

pub struct Player {
    pub n: V3,
    pub face: V3,
    pub vel: V3,
    pub alt: f32,
    pub alt_v: f32,
    pub grounded: bool,
    pub radius: f32,
    /// The rotation the last move applied, for the camera to follow.
    pub moved: (V3, f32),
    phase: f32,
    stride: f32,
    squash: f32,
    squash_v: f32,
    lean: f32,
    roll: f32,
    hair: [f32; 2],
    hair_v: [f32; 2],
    scarf: [V3; SCARF],
    scarf_prev: [V3; SCARF],
    blink: f32,
    next_blink: f32,
    /// Arms up, 0..1: held by the birds.
    pub reach: f32,
    pub reach_target: f32,
    /// Happiness after something good happens; fades.
    pub cheer: f32,
    pub events: Vec<Ev>,
}

fn spring(x: &mut f32, v: &mut f32, target: f32, k: f32, d: f32, dt: f32) {
    *v += ((target - *x) * k - *v * d) * dt;
    *x += *v * dt;
}

impl Player {
    pub fn new(n: V3, radius: f32) -> Player {
        let mut p = Player {
            n,
            face: any_perp(n),
            vel: V3::ZERO,
            alt: 0.0,
            alt_v: 0.0,
            grounded: true,
            radius,
            moved: (V3::Y, 0.0),
            phase: 0.0,
            stride: 0.0,
            squash: 0.0,
            squash_v: 0.0,
            lean: 0.0,
            roll: 0.0,
            hair: [0.0; 2],
            hair_v: [0.0; 2],
            scarf: [V3::ZERO; SCARF],
            scarf_prev: [V3::ZERO; SCARF],
            blink: 1.0,
            next_blink: 2.0,
            reach: 0.0,
            reach_target: 0.0,
            cheer: 0.0,
            events: Vec::new(),
        };
        p.reset_scarf();
        p
    }

    /// Puts the prince at `n`, `alt` above the ground, facing `face`.
    pub fn teleport(&mut self, n: V3, face: V3, radius: f32, alt: f32) {
        self.n = n.norm();
        self.face = face.flatten(self.n).norm();
        self.vel = V3::ZERO;
        self.radius = radius;
        self.alt = alt;
        self.alt_v = 0.0;
        self.grounded = alt <= 0.0;
        self.reset_scarf();
    }

    fn reset_scarf(&mut self) {
        let a = self.anchor();
        let back = -self.face;
        for i in 0..SCARF {
            self.scarf[i] = a + back * (SEG * i as f32 * 0.8) - self.n * (SEG * i as f32 * 0.5);
        }
        self.scarf_prev = self.scarf;
    }

    /// Knocked back by a trouble at `from`: a hop away from it.
    pub fn knock(&mut self, from: V3) {
        let away = (self.feet() - from).flatten(self.n);
        let away = if away.len() < 1e-3 { -self.face } else { away.norm() };
        self.vel = away * 2.6;
        self.alt_v = 2.2;
        self.grounded = false;
    }

    pub fn feet(&self) -> V3 {
        self.n * (self.radius + self.alt)
    }

    pub fn ground_point(&self) -> V3 {
        self.n * self.radius
    }

    pub fn right(&self) -> V3 {
        self.n.cross(self.face)
    }

    pub fn speed(&self) -> f32 {
        self.vel.len()
    }

    /// His frame at the feet with lean, roll and squash, at unit scale.
    pub fn pose_frame(&self) -> Frame {
        let b = self.body();
        let k = 1.0 / S;
        Frame { o: b.o, x: b.x * k, y: b.y * k, z: b.z * k }
    }

    fn body(&self) -> Frame {
        let up = self.n;
        let f = Frame { o: self.feet(), x: self.right() * S, y: up * S, z: self.face * S };
        let f = f.turned(0, -self.lean).turned(2, self.roll);
        let walk = self.stride.min(1.0);
        let bob = self.phase.sin().abs() * 0.025 * walk;
        let s = self.squash;
        Frame { o: f.o + up * (bob * S), x: f.x * (1.0 - s * 0.5), y: f.y * (1.0 + s), z: f.z * (1.0 - s * 0.5) }
    }

    fn anchor(&self) -> V3 {
        self.body().at(v3(0.0, 0.76, -0.055))
    }

    /// Where his hands are when raised: for the birds' strings.
    pub fn hands_up(&self) -> V3 {
        self.body().at(v3(0.0, 1.05, 0.06))
    }

    /// `wish` is the wanted direction along the ground (length up to 1).
    /// With `scripted` the caller drives `alt` (the flight between planets).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        dt: f32,
        wish: V3,
        jump: bool,
        surface: impl Fn(V3) -> f32,
        scripted: bool,
        wind: V3,
        time: f32,
    ) {
        let dt = dt.max(1e-4);
        let up = self.n;
        let prev = self.vel;
        let target = wish.flatten(up) * SPEED;
        let accel = if self.grounded { 10.0 } else { 4.0 };
        self.vel = approach3(self.vel, target, accel, dt).flatten(up);
        let sp = self.vel.len();
        self.moved = (up, 0.0);
        if sp > 1e-4 {
            let ang = sp * dt / (self.radius + self.alt);
            let axis = up.cross(self.vel).norm();
            self.n = self.n.rotate(axis, ang).norm();
            self.vel = self.vel.rotate(axis, ang);
            self.face = self.face.rotate(axis, ang);
            for i in 0..SCARF {
                self.scarf[i] = self.scarf[i].rotate(axis, ang);
                self.scarf_prev[i] = self.scarf_prev[i].rotate(axis, ang);
            }
            self.moved = (axis, ang);
        }
        let up = self.n;
        self.face = self.face.flatten(up).norm();
        let mut turn = 0.0;
        if sp > 0.15 {
            let want = self.vel.norm();
            let angle = self.face.cross(want).dot(up).atan2(self.face.dot(want));
            let step = angle * (1.0 - (-11.0 * dt).exp());
            self.face = self.face.rotate(up, step).norm();
            turn = step / dt;
        }
        let acc = (self.vel - prev) * (1.0 / dt);
        let acc_f = acc.dot(self.face);
        let acc_r = acc.dot(self.right());
        self.lean = approach(self.lean, (sp / SPEED) * 0.16 + acc_f * 0.012, 10.0, dt);
        self.roll = approach(self.roll, (turn * sp * 0.05).clamp(-0.3, 0.3), 8.0, dt);
        self.stride = approach(self.stride, (sp / SPEED).min(1.0), 8.0, dt);

        self.radius = surface(self.n);
        if !scripted {
            if jump && self.grounded {
                self.alt_v = JUMP;
                self.grounded = false;
                self.squash_v += 5.0;
                self.events.push(Ev::Jump(self.feet()));
            }
            if !self.grounded {
                self.alt_v -= GRAVITY * dt;
                self.alt += self.alt_v * dt;
                if self.alt <= 0.0 {
                    self.land(-self.alt_v);
                }
            }
        }
        if self.grounded {
            let before = self.phase;
            self.phase += sp * 7.5 * dt;
            if (before / PI).floor() != (self.phase / PI).floor() && sp > 0.4 {
                self.events.push(Ev::Step(self.ground_point()));
            }
        }
        spring(&mut self.squash, &mut self.squash_v, 0.0, 220.0, 11.0, dt);
        self.squash = self.squash.clamp(-0.35, 0.35);
        let hx = -(sp / SPEED) * 0.5 - acc_f * 0.03 + self.alt_v * 0.06;
        let hz = acc_r * 0.03 + self.roll;
        spring(&mut self.hair[0], &mut self.hair_v[0], hx.clamp(-0.9, 0.9), 120.0, 7.0, dt);
        spring(&mut self.hair[1], &mut self.hair_v[1], hz.clamp(-0.7, 0.7), 120.0, 7.0, dt);
        self.reach = approach(self.reach, self.reach_target.max(self.cheer * 0.55), 6.0, dt);
        self.cheer = (self.cheer - dt * 0.7).max(0.0);
        self.update_scarf(dt, wind, time);

        self.next_blink -= dt;
        if self.next_blink < 0.0 {
            self.blink = 0.0;
            self.next_blink = 2.0 + (time * 7.3).sin().abs() * 3.0;
        }
        self.blink = approach(self.blink, 1.0, 14.0, dt);
    }

    /// The scarf: a verlet rope pinned at the back of his neck, pushed by
    /// the wind and kept off his body and the ground.
    fn update_scarf(&mut self, dt: f32, wind: V3, time: f32) {
        let up = self.n;
        let anchor = self.anchor();
        let chest = self.body().at(v3(0.0, 0.6, 0.0));
        let steps = ((dt * 120.0).ceil() as usize).clamp(1, 12);
        let h = dt / steps as f32;
        for s in 0..steps {
            let tt = time + s as f32 * h;
            self.scarf[0] = anchor;
            self.scarf_prev[0] = anchor;
            for i in 1..SCARF {
                let p = self.scarf[i];
                let v = (p - self.scarf_prev[i]) * 0.965;
                self.scarf_prev[i] = p;
                let gust = 1.4 + 1.1 * (tt * 2.7 + i as f32 * 0.55).sin() + 0.6 * (tt * 6.1 - i as f32).sin();
                let acc = -up * 2.2 + wind * gust + self.right() * ((tt * 3.3 + i as f32 * 0.8).sin() * 0.8);
                self.scarf[i] = p + v + acc * (h * h);
            }
            for _ in 0..3 {
                for i in 1..SCARF {
                    let d = self.scarf[i] - self.scarf[i - 1];
                    self.scarf[i] = self.scarf[i - 1] + d.norm() * SEG;
                }
                for i in 2..SCARF {
                    let d = self.scarf[i] - chest;
                    if d.len() < 0.115 * S {
                        self.scarf[i] = chest + d.norm() * (0.115 * S);
                    }
                    let r = self.radius + 0.015;
                    if self.scarf[i].len() < r {
                        self.scarf[i] = self.scarf[i].norm() * r;
                    }
                }
            }
        }
        if !self.scarf[SCARF - 1].x.is_finite() {
            self.reset_scarf();
        }
    }

    pub fn land(&mut self, speed: f32) {
        self.alt = 0.0;
        self.alt_v = 0.0;
        self.grounded = true;
        self.squash_v -= speed.min(10.0) * 1.4;
        self.events.push(Ev::Land(self.ground_point(), speed));
    }

    /// A knee or elbow for a two-bone limb from `a` to `b` with lengths
    /// `l1` and `l2`, bending towards `bend`.
    fn joint(a: V3, b: V3, l1: f32, l2: f32, bend: V3) -> (V3, V3) {
        let d = b - a;
        let len = d.len().clamp(1e-4, (l1 + l2) * 0.999);
        let axis = d.norm();
        let b = a + axis * len;
        let x = (l1 * l1 - l2 * l2 + len * len) / (2.0 * len);
        let h = (l1 * l1 - x * x).max(0.0).sqrt();
        let side = bend.flatten(axis).norm();
        (a + axis * x + side * h, b)
    }

    pub fn draw(&self, gl: &Gl, kit: &Kit, time: f32) {
        let suit = rgb(0x5E9F55);
        let suit_dark = rgb(0x4A8546);
        let pants = rgb(0x6FAE60);
        let skin = rgb(0xFFE2CB);
        let blush = rgb(0xFFB3A7);
        let hair = rgb(0xF7C640);
        let hair_dark = rgb(0xE5A52E);
        let boot = rgb(0x7A4F33);
        let sole = rgb(0x4A3226);
        let gold = rgb(0xF2C14E);
        let belt = rgb(0xF0A833);
        let scarf = rgb(0xF3B23B);
        let eye = rgb(0x2B2133);
        let walk = self.stride.min(1.0);
        let air = !self.grounded;
        let r = Ink::Radial;
        let no = Ink::None;
        let (up, face, right) = (self.n, self.face, self.right());

        let ground = Frame { o: self.feet(), x: right * S, y: up * S, z: face * S };
        let b = self.body();
        // Legs: two bones each, the knee bending forward, feet planted in turn.
        for (side, ph) in [(-1.0f32, 0.0f32), (1.0, PI)] {
            let a = self.phase + ph;
            let lift = if air { 0.09 } else { a.cos().max(0.0) * 0.09 * walk };
            let fwd = if air { -side * 0.02 + 0.04 } else { a.sin() * 0.13 * walk };
            let ankle = ground.at(v3(side * 0.055, 0.065 + lift, fwd));
            let hip = b.at(v3(side * 0.05, 0.47, 0.0));
            let (knee, ankle) = Player::joint(hip, ankle, 0.21 * S, 0.21 * S, face + up * 0.1);
            kit.taper(gl, hip, knee, 0.05 * S, 0.042 * S, pants);
            kit.taper(gl, knee, ankle, 0.042 * S, 0.034 * S, pants);
            gl.part(kit.ball, place(knee, up, face, v3(0.042, 0.042, 0.042) * S), pants, 0.0, r);
            // The boot: a cuff, the foot and a darker sole.
            let toe = ankle + face * (0.03 * S) - up * (0.03 * S);
            gl.part(kit.stub, place(ankle - up * (0.035 * S), up, face, v3(0.042, 0.06, 0.042) * S), boot, 0.0, r);
            gl.part(kit.ball, place(toe, up, face, v3(0.042, 0.035, 0.07) * S), boot, 0.0, r);
            gl.part(kit.ball, place(toe - up * (0.026 * S), up, face, v3(0.044, 0.01, 0.072) * S), sole, 0.0, no);
        }
        // The suit: a flared coat skirt, the chest, a belt with a buckle, buttons and epaulettes.
        let swish = (self.phase * 2.0).sin() * 0.05 * walk;
        gl.part(kit.taper, b.turned(2, swish).part(v3(0.0, 0.37, 0.0), v3(0.125, 0.16, 0.11)), suit, 0.0, r);
        gl.part(kit.ball, b.part(v3(0.0, 0.6, 0.0), v3(0.1, 0.15, 0.08)), suit, 0.0, r);
        gl.part(kit.tube, b.part(v3(0.0, 0.485, 0.0), v3(0.1, 0.04, 0.086)), belt, 0.05, r);
        gl.part(kit.cube, b.part(v3(0.0, 0.505, 0.087), v3(0.022, 0.018, 0.006)), gold, 0.2, no);
        for y in [0.56f32, 0.62, 0.68] {
            gl.part(kit.ball, b.part(v3(0.0, y, 0.077), v3(0.012, 0.012, 0.008)), gold, 0.2, no);
        }
        gl.part(kit.cube, b.part(v3(0.0, 0.62, 0.079), v3(0.004, 0.09, 0.002)), suit_dark, 0.0, no);
        for side in [-1.0f32, 1.0] {
            gl.part(kit.ball, b.part(v3(side * 0.1, 0.735, 0.0), v3(0.048, 0.016, 0.042)), gold, 0.1, r);
        }
        // Arms: two bones, the elbow bending back; raised with the birds or with joy.
        let swing = self.phase.sin() * 0.12 * walk;
        let wave = (time * 9.0).sin() * 0.04 * self.cheer;
        for side in [-1.0f32, 1.0] {
            let shoulder = b.at(v3(side * 0.11, 0.705, 0.0));
            let rest = b.at(v3(side * 0.14, 0.43, -side * swing + 0.02));
            let raised = b.at(v3(side * (0.07 + wave), 1.05, 0.06));
            let hand = rest.lerp(raised, smooth(self.reach));
            let bend = -face + right * side * 0.4;
            let (elbow, hand) = Player::joint(shoulder, hand, 0.15 * S, 0.15 * S, bend);
            kit.taper(gl, shoulder, elbow, 0.036 * S, 0.032 * S, suit);
            kit.taper(gl, elbow, hand, 0.032 * S, 0.03 * S, suit);
            gl.part(kit.ball, place(elbow, up, face, v3(0.032, 0.032, 0.032) * S), suit, 0.0, r);
            let wrist = elbow.lerp(hand, 0.85);
            gl.part(kit.ball, place(wrist, hand - elbow, face, v3(0.036, 0.016, 0.036) * S), gold, 0.1, no);
            gl.part(
                kit.ball,
                place(hand + (hand - elbow).norm() * (0.012 * S), hand - elbow, face, v3(0.03, 0.036, 0.026) * S),
                skin,
                0.0,
                r,
            );
        }
        // The neck, wrapped in the scarf.
        gl.part(kit.tube, b.part(v3(0.0, 0.73, 0.0), v3(0.035, 0.06, 0.035)), skin, 0.0, no);
        gl.part(kit.ball, b.part(v3(0.0, 0.76, 0.0), v3(0.07, 0.035, 0.065)), scarf, 0.0, r);

        // The head: glancing about when he stands still, nodding when happy.
        let glance = (time * 0.5).sin() * 0.45 * (1.0 - walk);
        let nod = (time * 1.3).sin() * 0.04 * (1.0 - walk) - self.cheer * 0.12;
        let head = b.moved(v3(0.0, 0.875, 0.0)).turned(1, glance).turned(0, nod);
        gl.part(kit.ball, head.part(V3::ZERO, v3(0.1, 0.108, 0.098)), skin, 0.0, r);
        gl.part(kit.ball, head.part(v3(0.0, -0.045, 0.03), v3(0.08, 0.06, 0.07)), skin, 0.0, no);
        for side in [-1.0f32, 1.0] {
            gl.part(kit.ball, head.part(v3(side * 0.098, -0.005, -0.005), v3(0.016, 0.028, 0.02)), skin, 0.0, r);
            let open = self.blink.max(0.1) * if self.cheer > 0.3 { 0.5 } else { 1.0 };
            gl.part(kit.ball, head.part(v3(side * 0.037, -0.002, 0.088), v3(0.014, 0.021 * open, 0.01)), eye, 0.0, no);
            if open > 0.5 {
                gl.part(
                    kit.ball,
                    head.part(v3(side * 0.033, 0.007, 0.097), v3(0.005, 0.005, 0.003)),
                    rgb(0xFFFFFF),
                    0.8,
                    no,
                );
            }
            gl.part(kit.ball, head.part(v3(side * 0.06, -0.035, 0.078), v3(0.018, 0.01, 0.006)), blush, 0.1, no);
            gl.part(kit.ball, head.part(v3(side * 0.04, 0.036, 0.09), v3(0.017, 0.004, 0.005)), hair_dark, 0.0, no);
        }
        gl.part(kit.ball, head.part(v3(0.0, -0.022, 0.1), v3(0.011, 0.011, 0.01)), skin, 0.0, no);
        let smile = 0.004 + 0.006 * self.cheer;
        gl.part(kit.ball, head.part(v3(0.0, -0.055, 0.092), v3(0.016, smile, 0.005)), rgb(0xC0645E), 0.0, no);
        // Golden hair: a cap over the crown and curling tufts on springs, the
        // fringe swept over his forehead.
        gl.part(kit.ball, head.part(v3(0.0, 0.032, -0.018), v3(0.113, 0.1, 0.112)), hair, 0.0, r);
        let rings: [(f32, usize, f32); 4] = [(0.1, 10, 0.06), (0.5, 9, 0.065), (0.9, 7, 0.06), (1.3, 4, 0.055)];
        let mut k = 0;
        for (el, count, len) in rings {
            for i in 0..count {
                k += 1;
                let az = i as f32 / count as f32 * TAU + el * 1.7;
                let front = az.cos();
                if el < 0.3 && front > 0.35 {
                    continue;
                }
                let d = v3(el.cos() * az.sin(), el.sin(), el.cos() * front);
                let bounce = (time * 2.2 + k as f32).sin() * 0.05;
                let sweep = if el < 0.7 && front > 0.0 {
                    v3(0.3 * az.sin().signum(), -0.6, 0.25)
                } else {
                    v3(0.0, -0.2 + 0.15 * el, -0.35)
                };
                let spring = v3(self.hair[1] * 0.5, bounce, self.hair[0] * 0.5) * el.sin().max(0.25);
                let dir = head.dir(d + sweep + spring).norm();
                let base = head.at(d * 0.1 + v3(0.0, 0.032, -0.018));
                let col = if k % 3 == 0 { hair_dark } else { hair };
                let l = len * (0.85 + 0.3 * ((k * 7) % 5) as f32 / 5.0);
                gl.part(kit.cone, place(base, dir, head.z, v3(0.042, l, 0.03) * S), col, 0.0, r);
                // The tip curls on round.
                let axis = if k % 3 == 0 { head.x.norm() } else { dir.cross(up).norm() };
                let curl = dir.rotate(axis, -1.0);
                gl.part(
                    kit.cone,
                    place(base + dir * (l * 0.55 * S), curl, head.z, v3(0.026, l * 0.65, 0.02) * S),
                    col,
                    0.0,
                    r,
                );
            }
        }
    }

    /// The scarf's tail, shaded on the CPU and drawn through rlgl.
    pub fn draw_scarf(&self, l: &Lighting, time: f32, ink: raylib_sys::Color) {
        let col = rgb(0xF4C443);
        let side0 = self.right();
        let mut edges = Vec::with_capacity(SCARF);
        for i in 0..SCARF {
            let p = self.scarf[i];
            let dir = if i + 1 < SCARF { self.scarf[i + 1] - p } else { p - self.scarf[i - 1] }.norm();
            let k = i as f32 / SCARF as f32;
            let twist = (time * 5.0 + i as f32 * 0.6).sin() * 0.5 * k;
            let side = side0.rotate(dir, twist).flatten(dir).norm();
            let w = S * 0.03 * (1.0 - 0.25 * k);
            edges.push((p - side * w, p + side * w));
        }
        unsafe {
            ffi::rlDisableBackfaceCulling();
            ffi::rlBegin(ffi::RL_TRIANGLES as i32);
            for i in 1..SCARF {
                let (pa, pb) = edges[i - 1];
                let (a, b) = edges[i];
                let n = (pb - pa).cross(a - pa).norm();
                let c = l.shade_cpu(col, n, self.n);
                ffi::rlColor4ub(c.r, c.g, c.b, 255);
                for p in [pa, a, b, pa, b, pb] {
                    ffi::rlVertex3f(p.x, p.y, p.z);
                }
            }
            ffi::rlEnd();
            // Ink along both edges and the tip.
            ffi::rlBegin(ffi::RL_LINES as i32);
            ffi::rlColor4ub(ink.r, ink.g, ink.b, 200);
            for i in 1..SCARF {
                for (p, q) in [(edges[i - 1].0, edges[i].0), (edges[i - 1].1, edges[i].1)] {
                    ffi::rlVertex3f(p.x, p.y, p.z);
                    ffi::rlVertex3f(q.x, q.y, q.z);
                }
            }
            let (a, b) = edges[SCARF - 1];
            ffi::rlVertex3f(a.x, a.y, a.z);
            ffi::rlVertex3f(b.x, b.y, b.z);
            ffi::rlEnd();
            ffi::rlDrawRenderBatchActive();
            ffi::rlEnableBackfaceCulling();
        }
    }

    /// The soft shadow under him; inside `gl::alpha`.
    pub fn draw_shadow(&self) {
        let k = 1.0 / (1.0 + self.alt * 1.6);
        let r = self.right() * ((0.17 * k + 0.04) * S);
        let f = self.face * ((0.17 * k + 0.04) * S);
        Sprites::quad(self.ground_point() + self.n * 0.012, r, f, rgba(rgb(0x2A1E3A), 0.4 * k));
    }

    /// A few sparkles around him while he is happy; inside `gl::additive`.
    pub fn draw_glow(&self, s: &Sprites, t: f32) {
        if self.cheer < 0.05 {
            return;
        }
        let c = self.body().at(v3(0.0, 0.7, 0.0));
        for k in 0..5 {
            let a = t * 2.5 + k as f32 / 5.0 * TAU;
            let p = c
                + (self.right() * a.cos() + self.face * a.sin()) * 0.28
                + self.n * ((t * 3.0 + k as f32).sin() * 0.08);
            s.billboard(p, 0.05, rgba(rgb(0xFFE9A0), self.cheer));
        }
    }
}
