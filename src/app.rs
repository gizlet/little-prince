//! The app: the title, walking a planet and the flight between planets,
//! carried by migrating birds as in the book; the follow camera, the demo
//! autopilot, and turning the prince's steps, jumps and landings into dust,
//! rings and sound.

use crate::fx::{Burst, Fx};
use crate::gl::{self, lighten, mix, rgb, rgba, Gl, Ink, Lighting, Post, Sprites};
use crate::input::{Button, Controls};
use crate::math::{any_perp, approach3, clamp01, place, smooth, v3, Rng, V3};
use crate::hero::{Hero, Motion};
use crate::mesh::Kit;
use crate::player::{Ev, Player, SPEED};
use crate::sound::{Sfx, Sounds};
use crate::stats::Stats;
use crate::text::{Fonts, Lang};
use crate::world::{Kind, PState, World, KINDS};
use raylib_sys::{self as ffi, Camera3D, Color, Rectangle};
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Title,
    Play,
    Fly,
}

const LEAVE: f32 = 1.9;
const ARRIVE: f32 = 2.2;
const BIRDS: usize = 9;
/// The default hero model and its height in world units.
const HERO: &str = "Rogue_Hooded";
pub const HERO_H: f32 = 0.42;
/// Seconds on the title before the demo starts by itself.
const IDLE_DEMO: f32 = 12.0;
/// Seconds the demo spends on each planet.
const DEMO_STAY: f32 = 26.0;

const ANY: [Button; 12] = [
    Button::A,
    Button::B,
    Button::X,
    Button::Y,
    Button::Start,
    Button::Select,
    Button::L1,
    Button::R1,
    Button::Up,
    Button::Down,
    Button::Left,
    Button::Right,
];

pub struct Cam {
    pub pos: V3,
    pub target: V3,
    pub up: V3,
}

pub struct App {
    pub gl: Gl,
    kit: Kit,
    pub worlds: Vec<World>,
    pub cur: usize,
    pub player: Player,
    pub fx: Fx,
    snd: Sounds,
    pub fonts: Fonts,
    pub stats: Stats,
    pub mode: Mode,
    /// Seconds in the current mode.
    pub mode_t: f32,
    pub banner_t: f32,
    planet_t: f32,
    pub t: f32,
    pub idle_t: f32,
    pub lang: Lang,
    pub show_stats: bool,
    pub hud: bool,
    /// The rigged hero, or None to draw the procedural prince.
    pub hero: Option<Hero>,
    /// A front close-up of the prince, for checking the model.
    pub closeup: bool,
    /// 2 high (bloom), 1 mid (paper and lens), 0 low (none).
    pub fx_level: u8,
    pub demo: bool,
    pub quit_hold: f32,
    pub quit: bool,
    cam: Cam,
    cam_fwd: V3,
    fly_to: usize,
    switched: bool,
    demo_dir: V3,
    demo_jump: f32,
    rng: Rng,
}

impl App {
    pub fn new(lang: Lang, zoom: f32, audio: bool) -> App {
        let worlds: Vec<World> = KINDS.iter().map(|&k| World::new(k)).collect();
        let w = &worlds[0];
        let mut player = Player::new(w.spawn, w.surface(w.spawn));
        player.teleport(w.spawn, any_perp(w.spawn), w.surface(w.spawn), 0.0);
        let cam_fwd = player.face;
        let p = player.feet();
        let gl = Gl::new(std::env::var("LITTLEPRINCE_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0));
        let kit = Kit::new();
        gl.set_rig(kit.rigs.clone());
        App {
            gl,
            kit,
            player,
            cur: 0,
            worlds,
            fx: Fx::new(),
            snd: Sounds::new(audio),
            fonts: Fonts::new(zoom),
            stats: Stats::new(),
            mode: Mode::Title,
            mode_t: 0.0,
            banner_t: 0.0,
            planet_t: 0.0,
            t: 0.0,
            idle_t: 0.0,
            lang,
            show_stats: !std::env::var("LITTLEPRINCE_STATS").is_ok_and(|v| v == "0"),
            hud: true,
            hero: Hero::load(&std::env::var("LITTLEPRINCE_HERO").unwrap_or_else(|_| HERO.to_string())),
            closeup: false,
            fx_level: std::env::var("LITTLEPRINCE_FX").ok().and_then(|v| v.parse().ok()).unwrap_or(if cfg!(feature = "gizlet-os") { 1 } else { 2 }).min(2),
            demo: false,
            quit_hold: 0.0,
            quit: false,
            cam: Cam { pos: p * 3.0, target: p, up: p.norm() },
            cam_fwd,
            fly_to: 0,
            switched: false,
            demo_dir: cam_fwd,
            demo_jump: 2.0,
            rng: Rng::new(11),
        }
    }

    pub fn world(&self) -> &World {
        &self.worlds[self.cur]
    }

    fn set_mode(&mut self, m: Mode) {
        self.mode = m;
        self.mode_t = 0.0;
    }

    fn arrive_at(&mut self, idx: usize, alt: f32) {
        self.cur = idx;
        let w = &self.worlds[idx];
        let (n, r) = (w.spawn, w.surface(w.spawn));
        self.player.teleport(n, any_perp(n), r, alt);
        self.cam_fwd = self.player.face;
        self.fx.clear();
    }

    /// Starts walking on planet `idx` (used by the title and captures).
    pub fn start_play(&mut self, idx: usize) {
        if idx != self.cur {
            self.arrive_at(idx, 0.0);
        }
        self.set_mode(Mode::Play);
        self.banner_t = 0.0;
        self.planet_t = 0.0;
    }

    pub fn begin_flight(&mut self) {
        if self.mode != Mode::Play {
            return;
        }
        self.set_mode(Mode::Fly);
        self.fly_to = (self.cur + 1) % self.worlds.len();
        self.switched = false;
        self.player.reach_target = 1.0;
        self.snd.play(Sfx::Whoosh, 1.0, 0.5);
        self.snd.play(Sfx::Chime(5), 1.0, 0.3);
    }

    /// How far into the starry crossing the flight is, 0..1.
    pub fn warp_amount(&self) -> f32 {
        if self.mode != Mode::Fly {
            return 0.0;
        }
        if self.mode_t < LEAVE {
            smooth((self.mode_t / LEAVE - 0.45) / 0.55) * 0.7
        } else {
            (1.0 - smooth((self.mode_t - LEAVE) / (ARRIVE * 0.6))) * 0.7
        }
    }

    /// How present the birds are, 0..1.
    fn birds(&self) -> f32 {
        if self.mode != Mode::Fly {
            return 0.0;
        }
        smooth(self.mode_t / 0.5) * (1.0 - smooth((self.mode_t - LEAVE - ARRIVE + 0.2) / 0.4))
    }

    fn autopilot(&mut self, dt: f32) -> (V3, bool) {
        let n = self.player.n;
        let p = self.player.feet();
        self.demo_dir = self.demo_dir.flatten(n).norm().rotate(n, (self.t * 0.35).sin() * dt);
        let (dir, arrive, still) = match self.world().demo_target(p) {
            Some((tp, a, still)) => ((tp - p).flatten(n), a, still),
            None => (self.demo_dir, 0.0, false),
        };
        let d = dir.len();
        let wish = if still && d < arrive {
            V3::ZERO
        } else if d < 0.05 {
            self.demo_dir
        } else {
            dir.norm() * if still && d < arrive + 0.4 { 0.5 } else { 1.0 }
        };
        self.demo_jump -= dt;
        let jump = !still && self.demo_jump < 0.0;
        if self.demo_jump < 0.0 {
            self.demo_jump = self.rng.range(2.5, 5.0);
        }
        (wish, jump)
    }

    pub fn update(&mut self, dt: f32, c: &Controls) {
        self.t += dt;
        self.mode_t += dt;
        self.banner_t += dt;
        self.planet_t += dt;
        self.stats.update(dt);
        let combo = c.down(Button::Select) && c.down(Button::Start);
        if combo {
            self.quit_hold += dt;
            self.quit |= self.quit_hold > 2.0;
        } else {
            self.quit_hold = 0.0;
        }
        let (mx, my) = c.movement();
        let any = ANY.iter().any(|b| c.pressed(*b)) || mx != 0.0 || my != 0.0;
        let mut input = !combo;
        if any {
            self.idle_t = 0.0;
            if self.demo {
                self.demo = false;
                input = false;
            }
        } else {
            self.idle_t += dt;
        }
        if input {
            if c.pressed(Button::Select) {
                self.show_stats = !self.show_stats;
            }
            if c.pressed(Button::Start) {
                self.lang = if self.lang == Lang::Zh { Lang::En } else { Lang::Zh };
            }
            if c.pressed(Button::Y) {
                self.fx_level = (self.fx_level + 2) % 3;
            }
        }

        let mut wish = V3::ZERO;
        let mut jump = false;
        let n = self.player.n;
        match self.mode {
            Mode::Title => {
                if input && c.pressed(Button::A) {
                    self.start_play(self.cur);
                } else if self.idle_t > IDLE_DEMO {
                    self.demo = true;
                    self.start_play(self.cur);
                }
            }
            Mode::Play => {
                if self.demo {
                    (wish, jump) = self.autopilot(dt);
                    if self.planet_t > DEMO_STAY {
                        self.begin_flight();
                    }
                } else if input {
                    if c.pressed(Button::B) {
                        self.set_mode(Mode::Title);
                    } else if c.pressed(Button::X) {
                        self.begin_flight();
                    }
                    wish = self.cam_fwd * -my + self.cam_fwd.cross(n) * mx;
                    jump = c.pressed(Button::A);
                    let yaw = (c.down(Button::L1) as i32 - c.down(Button::R1) as i32) as f32;
                    self.cam_fwd = self.cam_fwd.rotate(n, yaw * 2.0 * dt);
                }
            }
            Mode::Fly => self.update_flight(),
        }
        if self.mode != Mode::Play {
            wish = V3::ZERO;
            jump = false;
        }

        let t = self.t;
        let wind = self.world().sky(self.player.n, t).wind;
        let w = &self.worlds[self.cur];
        self.player.update(dt, wish, jump, |n| w.surface(n), self.mode == Mode::Fly, wind, t);
        let (axis, ang) = self.player.moved;
        let n = self.player.n;
        self.cam_fwd = self.cam_fwd.rotate(axis, ang).flatten(n).norm();
        let sp = self.player.speed();
        if sp > 0.3 {
            let want = self.player.vel.norm();
            if self.demo || want.dot(self.cam_fwd) > -0.2 {
                let rate = if self.demo { 1.4 } else { 0.9 } * (sp / SPEED);
                let angle = self.cam_fwd.cross(want).dot(n).atan2(self.cam_fwd.dot(want));
                self.cam_fwd = self.cam_fwd.rotate(n, angle * (1.0 - (-rate * dt).exp())).norm();
            }
        }
        self.player_effects();
        let motion = Motion { speed: sp, grounded: self.player.grounded, flying: self.mode == Mode::Fly, cheer: self.player.cheer };
        if let Some(h) = &mut self.hero {
            h.update(dt, &motion);
        }
        let ps = PState { feet: self.player.feet(), face: self.player.face, speed: sp };
        if self.worlds[self.cur].update(dt, t, &ps, &mut self.fx, &mut self.snd) {
            self.player.cheer = 1.0;
        }
        self.fx.update(dt);
        self.update_camera(dt);
    }

    fn update_flight(&mut self) {
        let ft = self.mode_t;
        if ft < LEAVE {
            let u = ft / LEAVE;
            self.player.alt = 0.4 * u + 13.0 * u * u * u;
            self.player.alt_v = (0.4 + 39.0 * u * u) / LEAVE;
            self.player.grounded = false;
        } else if !self.switched {
            self.switched = true;
            self.arrive_at(self.fly_to, 13.0);
            self.player.reach_target = 1.0;
        } else {
            let u = ((ft - LEAVE) / ARRIVE).min(1.0);
            let k = 1.0 - u;
            self.player.alt = 13.0 * k * k * k;
            self.player.alt_v = -39.0 * k * k / ARRIVE;
            self.player.grounded = false;
            if u >= 1.0 {
                self.player.land(3.5);
                self.player.reach_target = 0.0;
                self.player.cheer = 1.0;
                self.set_mode(Mode::Play);
                self.banner_t = 0.0;
                self.planet_t = 0.0;
            }
        }
    }

    fn player_effects(&mut self) {
        let kind = self.world().kind;
        let accent = self.world().sky(self.player.n, self.t).accent;
        let dust = match kind {
            Kind::B612 => rgb(0xD9CFA6),
            Kind::Lamp => rgb(0xCBBFB0),
            Kind::Desert => rgb(0xF0D3A0),
        };
        let cols = [dust, lighten(dust, 0.3)];
        let events: Vec<Ev> = self.player.events.drain(..).collect();
        for ev in events {
            match ev {
                Ev::Step(p) => {
                    let up = p.norm();
                    let n = if kind == Kind::Desert { 4 } else { 2 };
                    self.fx.burst(p + up * 0.02, up, puff(n, &cols, 0.5));
                    self.snd.play(Sfx::Step, self.rng.range(0.85, 1.2), 0.3);
                }
                Ev::Jump(p) => {
                    let up = p.norm();
                    self.fx.burst(p, up, puff(6, &cols, 0.8));
                    self.fx.ring(p, up, 0.35, 0.4, rgba(dust, 0.7));
                    self.snd.play(Sfx::Step, 1.6, 0.35);
                }
                Ev::Land(p, speed) => {
                    if let Some(h) = &mut self.hero {
                        h.land();
                    }
                    let up = p.norm();
                    let k = (speed / 3.0).min(2.0);
                    self.fx.burst(p + up * 0.02, up, puff((5.0 * k) as usize + 3, &cols, 0.6 + k * 0.4));
                    self.fx.ring(p, up, 0.4 + k * 0.3, 0.5, rgba(dust, 0.8));
                    if self.player.reach_target > 0.5 || speed > 3.0 {
                        self.fx.ring(p, up, 1.4, 1.0, accent);
                        self.fx.burst(p, up, Burst {
                            n: 30,
                            speed: (0.5, 1.8),
                            size: (0.03, 0.08),
                            life: (0.7, 1.5),
                            cols: &[accent, lighten(accent, 0.6), rgb(0xFFFFFF)],
                            glow: true,
                            grav: 0.8,
                            drag: 1.6,
                            aim: 0.5,
                        });
                        self.snd.play(Sfx::Chime(2), 1.0, 0.4);
                    }
                    self.fx.shake = self.fx.shake.max(k * 0.15);
                    self.snd.play(Sfx::Land, 1.0, (0.15 + 0.2 * k).min(0.7));
                }
            }
        }
    }

    fn update_camera(&mut self, dt: f32) {
        let n = self.player.n;
        let p = self.player.feet();
        let f = self.cam_fwd;
        let r = self.world().radius;
        let (pos, target, rate) = match self.mode {
            Mode::Title => {
                let spawn = self.world().spawn;
                let h = any_perp(spawn).rotate(spawn, 2.6 + self.t * 0.08);
                let target = spawn * (r + 0.55);
                (target + h * (r * 1.1 + 2.3) + spawn * 0.35, target, 3.0)
            }
            Mode::Play if self.closeup => {
                let side = f.cross(n);
                (p + n * 0.3 + f * 0.85 + side * 0.25, p + n * 0.22, 8.0)
            }
            Mode::Play => {
                let rate = 1.6 + 4.4 * smooth(self.mode_t / 1.8);
                (p + n * 0.8 - f * 2.2, p + n * 0.22 + f * 1.3, rate)
            }
            Mode::Fly => {
                let k = if self.mode_t < LEAVE { self.mode_t / LEAVE } else { 1.0 - clamp01((self.mode_t - LEAVE) / ARRIVE) };
                (p + n * (0.95 + k * 0.9) - f * (2.0 + k * 1.2), p + n * (0.5 + 0.2 * k), 60.0)
            }
        };
        self.cam.pos = approach3(self.cam.pos, pos, rate, dt);
        self.cam.target = approach3(self.cam.target, target, rate * 1.4, dt);
        self.cam.up = approach3(self.cam.up, n, 4.0, dt).norm();
    }

    /// The birds: where each one is and which way it flies.
    fn bird_spots(&self) -> Vec<(V3, V3)> {
        let k = self.birds();
        let n = self.player.n;
        let f = self.player.face;
        let r = self.player.right();
        let hand = if self.hero.is_some() { Hero::crown(&self.player.pose_frame(), HERO_H) } else { self.player.hands_up() };
        (0..BIRDS)
            .map(|i| {
                let i = i as f32;
                let a = i / BIRDS as f32 * TAU + self.t * 0.9;
                let rad = (0.28 + 0.07 * (self.t * 1.7 + i).sin()) * (0.4 + 0.6 * k);
                let h = (0.3 + 0.12 * (self.t * 2.3 + i * 1.3).sin()) * k;
                let pos = hand + n * h + (r * a.cos() + f * a.sin()) * rad;
                (pos, (r * -a.sin() + f * a.cos()).norm())
            })
            .collect()
    }

    fn draw_birds(&self, g: &Gl) {
        let k = self.birds();
        if k < 0.01 {
            return;
        }
        let n = self.player.n;
        let white = rgb(0xF7F3EA);
        for (i, (pos, fwd)) in self.bird_spots().into_iter().enumerate() {
            let flap = (self.t * 11.0 + i as f32).sin();
            let k = k * 1.7;
            g.part(self.kit.ball, place(pos, n, fwd, v3(0.02, 0.018, 0.045) * k), white, 0.1, Ink::Radial);
            g.part(self.kit.cone, place(pos + fwd * 0.04 * k, fwd, n, v3(0.008, 0.02, 0.008) * k), rgb(0xF2B04E), 0.0, Ink::None);
            for side in [-1.0f32, 1.0] {
                let wd = (fwd.cross(n) * side + n * flap * 0.9).norm();
                g.part(self.kit.ball, place(pos + wd * 0.038 * k, wd, fwd, v3(0.01, 0.042, 0.022) * k), white, 0.1, Ink::Radial);
            }
        }
    }

    fn draw_strings(&self, ink: Color) {
        let k = self.birds();
        if k < 0.01 {
            return;
        }
        let hand = if self.hero.is_some() { Hero::crown(&self.player.pose_frame(), HERO_H) } else { self.player.hands_up() };
        let c = rgba(ink, 0.7 * k);
        unsafe {
            ffi::rlBegin(ffi::RL_LINES as i32);
            ffi::rlColor4ub(c.r, c.g, c.b, c.a);
            for (pos, _) in self.bird_spots() {
                ffi::rlVertex3f(hand.x, hand.y, hand.z);
                ffi::rlVertex3f(pos.x, pos.y, pos.z);
            }
            ffi::rlEnd();
            ffi::rlDrawRenderBatchActive();
        }
    }

    /// Draws the 3D scene: into the 640×480 target, or, when `direct`,
    /// straight to the screen inside `BeginDrawing`.
    pub fn render(&self, direct: bool) {
        let w = self.world();
        let warp = self.warp_amount();
        let t = self.t;
        let sky = w.sky(self.player.n, t);
        let fwd = (self.cam.target - self.cam.pos).norm();
        let right = fwd.cross(self.cam.up).norm();
        let up = right.cross(fwd);
        let shake = self.fx.shake_offset(t, right, up);
        let camera = Camera3D {
            position: (self.cam.pos + shake).rl(),
            target: (self.cam.target + shake).rl(),
            up: self.cam.up.rl(),
            fovy: 50.0 + warp * 14.0,
            projection: ffi::CameraProjection::CAMERA_PERSPECTIVE as i32,
        };
        let spr = Sprites { right, up, fwd };
        let (lamp, lamp_on) = w.lamp_light().unwrap_or((v3(0.0, 1000.0, 0.0), 0.0));
        let fog = match w.kind {
            Kind::B612 => (5.0, 15.0),
            Kind::Desert => (5.0, 16.0),
            Kind::Lamp => (6.0, 20.0),
        };
        let light = Lighting {
            sun: sky.sun,
            sun_col: sky.sun_col,
            shade: sky.shade,
            sky: sky.amb,
            rim: sky.rim,
            fog: sky.fog,
            fog_range: fog,
            cam: self.cam.pos,
            pusher: self.player.feet(),
            wind: sky.wind,
            lamp,
            lamp_col: gl::scale(rgb(0xFFB45C), lamp_on * 1.4),
        };
        let top = mix(sky.top, rgb(0x070A22), warp);
        let bottom = mix(sky.bottom, rgb(0x1A1640), warp);
        let g = &self.gl;
        g.ink.set(sky.ink);
        g.hero_ink.set(self.fx_level >= 2);
        g.outlines.set(!std::env::var("LITTLEPRINCE_NOINK").is_ok_and(|v| v == "1"));
        unsafe {
            if !direct {
                ffi::BeginTextureMode(g.scene);
                ffi::ClearBackground(bottom);
            }
            ffi::BeginMode3D(camera);
        }
        g.light(&light, t);
        crate::prof::mark("setup");
        // Opaque things first, with blending off so the GPU can drop hidden
        // fragments early (Mali's forward pixel kill and early depth test
        // skip blended draws), nearest first: the hero, then the world.
        let opaque = !crate::prof::skip("blend");
        if opaque {
            unsafe {
                ffi::rlDrawRenderBatchActive();
                ffi::rlDisableColorBlend();
            }
        }
        w.draw(g, &self.kit, t, self.cam.pos, fwd);
        crate::prof::mark("world-queue");
        match &self.hero {
            Some(_) if crate::prof::skip("hero") => {}
            Some(h) => h.draw(g, &self.player.pose_frame(), HERO_H),
            None => self.player.draw(g, &self.kit, t),
        }
        self.draw_birds(g);
        crate::prof::mark("cpu-parts");
        g.flush();
        crate::prof::mark("solids");
        w.draw_grass(g, self.cam.pos, fwd);
        crate::prof::mark("grass");
        // The sky gradient last, on a quad at the far end of the view: only the
        // pixels the planet leaves uncovered are filled, not the whole screen.
        draw_backdrop(self.cam.pos + shake, fwd, right, up, 50.0 + warp * 14.0, top, bottom);
        if opaque {
            unsafe {
                ffi::rlDrawRenderBatchActive();
                ffi::rlEnableColorBlend();
            }
        }
        crate::prof::mark("backdrop");
        // Stars, sun, moon and clouds after the solids, so the depth test
        // rejects whatever the planet hides.
        gl::additive(|| w.draw_sky(g, &spr, self.cam.pos, t, warp, &sky));
        crate::prof::mark("sky");
        if self.hero.is_none() {
            self.player.draw_scarf(&light, t, sky.ink);
        }
        self.draw_strings(sky.ink);
        gl::alpha(|| {
            Sprites::begin(g.sprite);
            self.player.draw_shadow();
            Sprites::end();
            self.fx.draw_dust(&spr, g.sprite);
            w.draw_alpha(g, t, self.cam.pos);
        });
        crate::prof::mark("scarf+dust");
        w.draw_glass(g, &self.kit);
        gl::additive(|| {
            w.draw_glow(g, &spr, t);
            Sprites::begin(g.sprite);
            self.player.draw_glow(&spr, t);
            Sprites::end();
            self.fx.draw_glow(&spr, g.sprite, g.ring);
        });
        unsafe {
            ffi::EndMode3D();
            if direct {
                if self.fx.flash > 0.01 {
                    ffi::DrawRectangle(0, 0, gl::W, gl::H, rgba(rgb(0xFFFFFF), self.fx.flash));
                }
            } else {
                ffi::EndTextureMode();
            }
        }
        crate::prof::mark("glow");
    }

    /// Puts the scene on the screen with the post-processing chain.
    pub fn present(&self, dest: Rectangle) {
        let warp = self.warp_amount();
        let post = Post {
            level: self.fx_level,
            bloom: 0.4,
            threshold: 0.9,
            aberr: warp * 0.4,
            warp: warp * 0.8,
            flash: self.fx.flash,
        };
        self.gl.present(&post, dest);
    }
}

/// A camera-facing quad just inside the far plane, `top` colour at the top
/// of the screen fading to `bottom`.
fn draw_backdrop(cam: V3, fwd: V3, right: V3, up: V3, fovy: f32, top: Color, bottom: Color) {
    // Well inside the far plane: on a 16-bit depth buffer a backdrop near
    // it rounds to the cleared depth, fails the test and flickers black.
    let d = crate::world::SKY_D + 4.0;
    let h = d * (fovy.to_radians() * 0.5).tan() * 1.1;
    let w = h * (gl::W as f32 / gl::H as f32) * 1.1;
    let c = cam + fwd * d;
    unsafe {
        ffi::rlDisableBackfaceCulling();
        ffi::rlSetTexture(0);
        ffi::rlBegin(ffi::RL_QUADS as i32);
        for (p, col) in [(c - right * w + up * h, top), (c - right * w - up * h, bottom), (c + right * w - up * h, bottom), (c + right * w + up * h, top)] {
            ffi::rlColor4ub(col.r, col.g, col.b, 255);
            ffi::rlVertex3f(p.x, p.y, p.z);
        }
        ffi::rlEnd();
        ffi::rlDrawRenderBatchActive();
        ffi::rlEnableBackfaceCulling();
    }
}

fn puff(n: usize, cols: &[Color; 2], speed: f32) -> Burst<'_> {
    Burst { n, speed: (0.3 * speed, 0.9 * speed), size: (0.04, 0.08), life: (0.4, 0.8), cols, glow: false, grav: 1.5, drag: 3.0, aim: 0.55 }
}
