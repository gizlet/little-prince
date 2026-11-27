//! The app: the title, walking a planet and the flight between planets,
//! carried by migrating birds as in the book; the follow camera, the
//! autopilot, and turning the prince's steps, jumps and landings into dust,
//! rings and sound.

use crate::fx::{Burst, Fx};
use crate::gl::{self, lighten, mix, rgb, rgba, Gl, Ink, Lighting, Post, Sprites};
use crate::hero::{Hero, Motion};
use crate::input::{Button, Controls};
use crate::math::{any_perp, approach3, clamp01, place, smooth, v3, Rng, V3};
use crate::menu::{Action, Keys, Menu, Page, Setting};
use crate::mesh::Kit;
use crate::player::{Ev, Player, SPEED};
use crate::save::{Journey, Settings, Store, SLOTS};
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
/// The camera's resting height angle and its limits, in radians.
const PITCH: f32 = 0.26;
const PITCH_MIN: f32 = -0.2;
const PITCH_MAX: f32 = 1.2;
/// The default hero model and its height in world units.
const HERO: &str = "Rogue_Hooded";
pub const HERO_H: f32 = 0.42;
/// Seconds on the title before autoplay starts by itself.
const IDLE_AUTOPLAY: f32 = 12.0;
/// Seconds autoplay spends on each planet.
const AUTOPLAY_STAY: f32 = 26.0;

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
    pub planet_t: f32,
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
    pub autoplay: bool,
    pub quit_hold: f32,
    pub quit: bool,
    cam: Cam,
    cam_fwd: V3,
    /// The camera's height angle above the hero, in radians.
    cam_pitch: f32,
    /// Seconds since the player last turned the camera by hand.
    look_t: f32,
    fly_to: usize,
    switched: bool,
    auto_dir: V3,
    auto_jump: f32,
    rng: Rng,
    pub store: Store,
    pub settings: Settings,
    /// The journey being played; autoplay plays none and saves nothing.
    pub slot: Option<usize>,
    pub journey: Journey,
    /// The journeys on disk, for the journeys page.
    pub journeys: [Option<Journey>; SLOTS],
    /// The menu on screen; play stops while one is open.
    pub menu: Option<Menu>,
    /// Seconds since the journey was last saved.
    save_t: f32,
    /// The sun's height over the prince last frame, for counting sunsets.
    sun_e: f32,
}

/// Seconds between saves while playing, besides those on arriving, scoring,
/// opening the menu and quitting.
const AUTOSAVE: f32 = 60.0;

impl App {
    /// `lang` is the platform's language; a language chosen in the settings
    /// wins over it.
    pub fn new(lang: Lang, zoom: f32, audio: bool, store: Store) -> App {
        let settings = store.settings();
        let lang = match settings.lang.as_deref() {
            Some("en") => Lang::En,
            Some("zh") => Lang::Zh,
            _ => lang,
        };
        let fx_level = std::env::var("LITTLEPRINCE_FX")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(settings.fx)
            .unwrap_or(if cfg!(feature = "gizlet-os") { 1 } else { 2 })
            .min(2);
        let show_stats = match std::env::var("LITTLEPRINCE_STATS") {
            Ok(v) => v != "0",
            Err(_) => settings.stats.unwrap_or(true),
        };
        let mut snd = Sounds::new(audio);
        snd.volume = settings.volume as f32 / 10.0;
        let journeys = std::array::from_fn(|i| store.journey(i));
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
            snd,
            fonts: Fonts::new(zoom),
            stats: Stats::new(),
            mode: Mode::Title,
            mode_t: 0.0,
            planet_t: 0.0,
            t: 0.0,
            idle_t: 0.0,
            lang,
            show_stats,
            hud: true,
            hero: Hero::load(&std::env::var("LITTLEPRINCE_HERO").unwrap_or_else(|_| HERO.to_string())),
            closeup: false,
            fx_level,
            autoplay: false,
            quit_hold: 0.0,
            quit: false,
            cam: Cam { pos: p * 3.0, target: p, up: p.norm() },
            cam_fwd,
            cam_pitch: PITCH,
            look_t: 10.0,
            fly_to: 0,
            switched: false,
            auto_dir: cam_fwd,
            auto_jump: 2.0,
            rng: Rng::new(11),
            store,
            settings,
            slot: None,
            journey: Journey::start(),
            journeys,
            menu: None,
            save_t: 0.0,
            sun_e: 0.0,
        }
    }

    /// A value for `LITTLEPRINCE_SCRIPT` checks, by name (see `script`).
    pub fn probe(&self, name: &str) -> String {
        use crate::save::PLANETS;
        let st = &self.journey.stats;
        let planet = |name: &str| PLANETS.iter().position(|p| *p == name);
        match name {
            "mode" => format!("{:?}", self.mode).to_lowercase(),
            "menu" => self.menu.as_ref().map_or("none".into(), |m| format!("{:?}", m.top().0).to_lowercase()),
            "row" => self.menu.as_ref().map_or(0, |m| m.top().1).to_string(),
            "planet" => PLANETS[self.cur].into(),
            "lang" => if self.lang == Lang::Zh { "zh" } else { "en" }.into(),
            "slot" => self.slot.map_or("none".into(), |s| (s + 1).to_string()),
            "fx" => self.fx_level.to_string(),
            "volume" => self.settings.volume.to_string(),
            "invert" => (self.settings.invert as u8).to_string(),
            "stats-panel" => (self.show_stats as u8).to_string(),
            "autoplay" => (self.autoplay as u8).to_string(),
            "time" => format!("{:.2}", st.time),
            "distance" => format!("{:.2}", st.distance),
            "jumps" => st.jumps.to_string(),
            "sunsets" => st.sunsets.to_string(),
            "sprouts" => st.sprouts.to_string(),
            "flights" => st.flights.to_string(),
            "visited" => self.journey.visited.iter().filter(|v| **v).count().to_string(),
            "save-failed" => (self.store.failed as u8).to_string(),
            _ => {
                if let Some(p) = name.strip_prefix("score.").and_then(planet) {
                    return self.worlds[p].score.to_string();
                }
                // `saved.N` is whether journey N is on disk, `saved.N.KEY`
                // its planet or a statistic as written there.
                if let Some(rest) = name.strip_prefix("saved.") {
                    let (n, key) = rest.split_once('.').unwrap_or((rest, ""));
                    let j = n.parse::<usize>().ok().and_then(|n| self.store.journey(n.checked_sub(1)?));
                    return match (j, key) {
                        (None, "") => "0".into(),
                        (Some(_), "") => "1".into(),
                        (Some(j), "planet") => PLANETS[j.planet].into(),
                        (Some(j), "jumps") => j.stats.jumps.to_string(),
                        (Some(j), "flights") => j.stats.flights.to_string(),
                        (Some(j), "time") => format!("{:.2}", j.stats.time),
                        (Some(j), "distance") => format!("{:.2}", j.stats.distance),
                        (Some(j), "visited") => j.visited.iter().filter(|v| **v).count().to_string(),
                        (Some(j), k) => match k.strip_prefix("score.").and_then(planet) {
                            Some(p) => j.scores[p].to_string(),
                            None => format!("unknown {name}"),
                        },
                        (None, _) => "none".into(),
                    };
                }
                format!("unknown {name}")
            }
        }
    }

    /// Whether this is a journey being played by hand, whose progress counts.
    fn counting(&self) -> bool {
        self.slot.is_some() && !self.autoplay
    }

    /// Writes the journey being played, if any.
    pub fn save_now(&mut self) {
        let Some(slot) = self.slot else { return };
        self.journey.planet = self.cur;
        self.journey.visited[self.cur] = true;
        for (score, w) in self.journey.scores.iter_mut().zip(&self.worlds) {
            *score = w.score;
        }
        self.store.save_journey(slot, &self.journey);
        self.journeys[slot] = Some(self.journey.clone());
        self.save_t = 0.0;
    }

    /// Opens a menu page; the pause menu saves first.
    pub fn open_menu(&mut self, page: Page) {
        let row = match page {
            Page::Journeys => {
                self.journeys = std::array::from_fn(|i| self.store.journey(i));
                self.settings.slot
            }
            Page::Pause => {
                self.save_now();
                0
            }
            _ => 0,
        };
        self.menu = Some(Menu::open(page, row));
    }

    /// Starts or continues journey `slot`.
    fn load(&mut self, slot: usize) {
        let j = self.store.journey(slot).unwrap_or_else(Journey::start);
        for (w, score) in self.worlds.iter_mut().zip(j.scores) {
            w.restore(score);
        }
        self.menu = None;
        self.autoplay = false;
        self.slot = Some(slot);
        self.settings.slot = slot;
        self.store.save_settings(&self.settings);
        let planet = j.planet;
        self.journey = j;
        self.arrive_at(planet, 0.0);
        self.start_play(planet);
        self.save_now();
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Close => self.menu = None,
            Action::Load(slot) => self.load(slot),
            Action::Clear(slot) => {
                self.store.clear_journey(slot);
                self.journeys[slot] = None;
                self.snd.play(Sfx::Splash, 0.8, 0.4);
            }
            Action::SaveAndQuit => {
                self.save_now();
                self.slot = None;
                self.menu = None;
                self.set_mode(Mode::Title);
            }
            Action::Change(setting, step) => self.change(setting, step),
        }
    }

    fn change(&mut self, setting: Setting, step: i32) {
        let s = &mut self.settings;
        match setting {
            Setting::Language => {
                self.lang = if self.lang == Lang::Zh { Lang::En } else { Lang::Zh };
                s.lang = Some(if self.lang == Lang::Zh { "zh" } else { "en" }.into());
            }
            Setting::Effects => {
                self.fx_level = (self.fx_level as i32 + step).rem_euclid(3) as u8;
                s.fx = Some(self.fx_level);
            }
            Setting::Performance => {
                self.show_stats = !self.show_stats;
                s.stats = Some(self.show_stats);
            }
            Setting::Volume => {
                s.volume = (s.volume as i32 + step).clamp(0, 10) as u8;
                self.snd.volume = s.volume as f32 / 10.0;
            }
            Setting::Invert => s.invert = !s.invert,
        }
        self.snd.play(Sfx::Chime(3), 1.0, 0.4);
        self.store.save_settings(&self.settings);
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
        self.sun_e = 0.0;
        if self.counting() {
            self.journey.visited[idx] = true;
        }
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
        self.planet_t = 0.0;
    }

    pub fn begin_flight(&mut self) {
        if self.mode != Mode::Play {
            return;
        }
        self.set_mode(Mode::Fly);
        if self.counting() {
            self.journey.stats.flights += 1;
        }
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
        self.auto_dir = self.auto_dir.flatten(n).norm().rotate(n, (self.t * 0.35).sin() * dt);
        let (dir, arrive, still) = match self.world().autoplay_target(p) {
            Some((tp, a, still)) => ((tp - p).flatten(n), a, still),
            None => (self.auto_dir, 0.0, false),
        };
        let d = dir.len();
        let wish = if still && d < arrive {
            V3::ZERO
        } else if d < 0.05 {
            self.auto_dir
        } else {
            dir.norm() * if still && d < arrive + 0.4 { 0.5 } else { 1.0 }
        };
        self.auto_jump -= dt;
        let jump = !still && self.auto_jump < 0.0;
        if self.auto_jump < 0.0 {
            self.auto_jump = self.rng.range(2.5, 5.0);
        }
        (wish, jump)
    }

    pub fn update(&mut self, dt: f32, c: &Controls) {
        self.stats.update(dt);
        let combo = c.down(Button::Select) && c.down(Button::Start);
        if combo {
            self.quit_hold += dt;
            self.quit |= self.quit_hold > 2.0;
        } else {
            self.quit_hold = 0.0;
        }
        let (mx, my) = c.movement();
        let (lx, ly) = c.look();
        let ly = if self.settings.invert { -ly } else { ly };
        let any = ANY.iter().any(|b| c.pressed(*b)) || mx != 0.0 || my != 0.0 || lx != 0.0 || ly != 0.0;
        let mut input = !combo;
        if any {
            self.idle_t = 0.0;
            if self.autoplay {
                self.autoplay = false;
                input = false;
            }
        } else {
            self.idle_t += dt;
        }
        // A menu takes the buttons; over play, everything stands still.
        if let Some(mut menu) = self.menu.take() {
            let keys = Keys {
                up: c.nav(Button::Up),
                down: c.nav(Button::Down),
                left: c.nav(Button::Left),
                right: c.nav(Button::Right),
                a: c.pressed(Button::A),
                b: c.pressed(Button::B),
                x: c.pressed(Button::X),
                start: c.pressed(Button::Start),
            };
            let action = if input { menu.step(keys) } else { Action::None };
            self.menu = Some(menu);
            self.act(action);
            input = false;
            if self.mode != Mode::Title {
                return;
            }
        }
        self.t += dt;
        self.mode_t += dt;
        self.planet_t += dt;
        self.look_t += dt;
        if input {
            if c.pressed(Button::Y) {
                self.fx_level = (self.fx_level + 2) % 3;
                self.settings.fx = Some(self.fx_level);
                self.store.save_settings(&self.settings);
            }
            match self.mode {
                Mode::Title if c.pressed(Button::Start) => self.open_menu(Page::Settings),
                Mode::Title if c.pressed(Button::A) => self.open_menu(Page::Journeys),
                Mode::Play if !self.autoplay && c.pressed(Button::Start) => self.open_menu(Page::Pause),
                Mode::Play if !self.autoplay && c.pressed(Button::Select) => self.open_menu(Page::Statistics),
                _ => {}
            }
            if self.menu.is_some() {
                return;
            }
        }
        if self.counting() {
            if self.mode != Mode::Title {
                self.journey.stats.time += dt as f64;
            }
            self.save_t += dt;
            if self.save_t > AUTOSAVE {
                self.save_now();
            }
        }

        let mut wish = V3::ZERO;
        let mut jump = false;
        let n = self.player.n;
        match self.mode {
            Mode::Title => {
                if self.idle_t > IDLE_AUTOPLAY && self.menu.is_none() {
                    self.autoplay = true;
                    self.start_play(self.cur);
                }
            }
            Mode::Play => {
                if self.autoplay {
                    (wish, jump) = self.autopilot(dt);
                    if self.planet_t > AUTOPLAY_STAY {
                        self.begin_flight();
                    }
                } else if input {
                    if c.pressed(Button::X) {
                        self.begin_flight();
                    }
                    wish = self.cam_fwd * -my + self.cam_fwd.cross(n) * mx;
                    jump = c.pressed(Button::A);
                    // Right stick: left/right orbits the camera round the
                    // hero, up/down raises or lowers it (stick up looks up).
                    if lx != 0.0 || ly != 0.0 {
                        self.look_t = 0.0;
                        self.cam_fwd = self.cam_fwd.rotate(n, -lx * 2.6 * dt);
                        self.cam_pitch = (self.cam_pitch + ly * 1.6 * dt).clamp(PITCH_MIN, PITCH_MAX);
                    }
                }
            }
            Mode::Fly => self.update_flight(),
        }
        if self.mode != Mode::Play {
            wish = V3::ZERO;
            jump = false;
        }

        let t = self.t;
        let wind = self.world().wind();
        let w = &self.worlds[self.cur];
        self.player.update(dt, wish, jump, |n| w.surface(n), self.mode == Mode::Fly, wind, t);
        let (axis, ang) = self.player.moved;
        let n = self.player.n;
        self.cam_fwd = self.cam_fwd.rotate(axis, ang).flatten(n).norm();
        let sp = self.player.speed();
        if self.counting() && self.mode == Mode::Play && self.player.grounded {
            self.journey.stats.distance += (sp * dt) as f64;
        }
        if sp > 0.3 {
            let want = self.player.vel.norm();
            // Follow the walking direction, but not right after the player
            // turned the camera by hand.
            if self.autoplay || (want.dot(self.cam_fwd) > -0.2 && self.look_t > 1.5) {
                let rate = if self.autoplay { 1.4 } else { 0.9 } * (sp / SPEED);
                let angle = self.cam_fwd.cross(want).dot(n).atan2(self.cam_fwd.dot(want));
                self.cam_fwd = self.cam_fwd.rotate(n, angle * (1.0 - (-rate * dt).exp())).norm();
            }
        }
        self.player_effects();
        let motion = Motion {
            speed: sp,
            grounded: self.player.grounded,
            flying: self.mode == Mode::Fly,
            cheer: self.player.cheer,
        };
        if let Some(h) = &mut self.hero {
            h.update(dt, &motion);
        }
        let ps = PState { feet: self.player.feet(), face: self.player.face, speed: sp };
        let before = self.worlds[self.cur].score;
        let scored = self.worlds[self.cur].update(dt, t, &ps, &mut self.fx, &mut self.snd);
        if scored {
            self.player.cheer = 1.0;
        }
        // Sunsets wherever the prince stands, on any planet.
        let e = self.player.n.dot(self.world().sun(t));
        let sunset = self.mode == Mode::Play && self.sun_e > 0.0 && e <= 0.0;
        self.sun_e = e;
        if self.counting() {
            let w = &self.worlds[self.cur];
            if w.kind == Kind::B612 && w.score > before {
                self.journey.stats.sprouts += (w.score - before) as u32;
            }
            if sunset {
                self.journey.stats.sunsets += 1;
            }
            if scored {
                self.save_now();
            }
        }
        self.fx.update(dt);
        self.update_camera(dt);
    }

    /// How high the birds carry the prince: high enough to see the planet
    /// round below.
    fn flight_height(&self) -> f32 {
        (self.world().radius * 0.75).max(13.0)
    }

    fn update_flight(&mut self) {
        let ft = self.mode_t;
        if ft < LEAVE {
            let u = ft / LEAVE;
            let h = self.flight_height();
            self.player.alt = 0.4 * u + h * u * u * u;
            self.player.alt_v = (0.4 + 3.0 * h * u * u) / LEAVE;
            self.player.grounded = false;
        } else if !self.switched {
            self.switched = true;
            let h = (self.worlds[self.fly_to].radius * 0.75).max(13.0);
            self.arrive_at(self.fly_to, h);
            self.player.reach_target = 1.0;
        } else {
            let u = ((ft - LEAVE) / ARRIVE).min(1.0);
            let k = 1.0 - u;
            let h = self.flight_height();
            self.player.alt = h * k * k * k;
            self.player.alt_v = -3.0 * h * k * k / ARRIVE;
            self.player.grounded = false;
            if u >= 1.0 {
                self.player.land(3.5);
                self.player.reach_target = 0.0;
                self.player.cheer = 1.0;
                self.start_play(self.cur);
                self.save_now();
            }
        }
    }

    fn player_effects(&mut self) {
        let kind = self.world().kind;
        let accent = self.world().accent();
        let dust = match kind {
            Kind::B612 => rgb(0xD9CFA6),
            Kind::Lamp => rgb(0xCBBFB0),
            Kind::Desert => rgb(0xF0D3A0),
        };
        let cols = [dust, lighten(dust, 0.3)];
        let counting = self.counting();
        for ev in self.player.events.drain(..) {
            match ev {
                Ev::Step(p) => {
                    let up = p.norm();
                    let n = if kind == Kind::Desert { 4 } else { 2 };
                    self.fx.burst(p + up * 0.02, up, puff(n, &cols, 0.5));
                    self.snd.play(Sfx::Step, self.rng.range(0.85, 1.2), 0.3);
                }
                Ev::Jump(p) => {
                    if counting {
                        self.journey.stats.jumps += 1;
                    }
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
                        self.fx.burst(
                            p,
                            up,
                            Burst {
                                n: 30,
                                speed: (0.5, 1.8),
                                size: (0.03, 0.08),
                                life: (0.7, 1.5),
                                cols: &[accent, lighten(accent, 0.6), rgb(0xFFFFFF)],
                                glow: true,
                                grav: 0.8,
                                drag: 1.6,
                                aim: 0.5,
                            },
                        );
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
                let e = if self.autoplay { PITCH } else { self.cam_pitch };
                let pivot = p + n * 0.3;
                let mut pos = pivot + (n * e.sin() - f * e.cos()) * 2.3;
                // Looking down from above, aim at the hero; looking up from
                // low down, aim ahead and up at the sky.
                let ahead = 1.2 * e.cos().max(0.0) * (1.0 - smooth((e - 0.4) / 0.8));
                let target = p + n * (0.22 + (PITCH - e).max(0.0) * 2.5) + f * ahead;
                // Never below the ground.
                let floor = self.world().surface(pos.norm()) + 0.15;
                if pos.len() < floor {
                    pos = pos.norm() * floor;
                }
                (pos, target, rate.max(if self.look_t < 0.3 { 12.0 } else { 0.0 }))
            }
            Mode::Fly => {
                let k = if self.mode_t < LEAVE {
                    self.mode_t / LEAVE
                } else {
                    1.0 - clamp01((self.mode_t - LEAVE) / ARRIVE)
                };
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
        let hand =
            if self.hero.is_some() { Hero::crown(&self.player.pose_frame(), HERO_H) } else { self.player.hands_up() };
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
            g.part(
                self.kit.cone,
                place(pos + fwd * 0.04 * k, fwd, n, v3(0.008, 0.02, 0.008) * k),
                rgb(0xF2B04E),
                0.0,
                Ink::None,
            );
            for side in [-1.0f32, 1.0] {
                let wd = (fwd.cross(n) * side + n * flap * 0.9).norm();
                g.part(
                    self.kit.ball,
                    place(pos + wd * 0.038 * k, wd, fwd, v3(0.01, 0.042, 0.022) * k),
                    white,
                    0.1,
                    Ink::Radial,
                );
            }
        }
    }

    fn draw_strings(&self, ink: Color) {
        let k = self.birds();
        if k < 0.01 {
            return;
        }
        let hand =
            if self.hero.is_some() { Hero::crown(&self.player.pose_frame(), HERO_H) } else { self.player.hands_up() };
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
        // The haze starts a little beyond the ground the camera looks down
        // on, so the planet stays clear from the title's height too.
        let lift = (self.cam.pos.len() - w.surface(self.cam.pos.norm()) - 1.0).max(0.0);
        let (near, far) = match w.kind {
            Kind::B612 => (6.0, 18.0),
            Kind::Desert => (7.0, 22.0),
            Kind::Lamp => (6.0, 20.0),
        };
        let fog = (near + lift, far + lift * 2.0);
        let sky_d = w.sky_distance(self.cam.pos);
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
            wind: w.wind(),
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
        // The title looks at the planet from afar, with nothing else to draw.
        let detail = if self.mode == Mode::Title { 1.6 } else { 1.0 };
        w.draw(g, &self.kit, t, self.cam.pos, fwd, detail);
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
        // The sky gradient last, on a quad at the far end of the view: only the
        // pixels the planet leaves uncovered are filled, not the whole screen.
        if !crate::prof::skip("backdrop") {
            draw_backdrop(self.cam.pos + shake, fwd, right, up, 50.0 + warp * 14.0, top, bottom, sky_d + 4.0);
        }
        if opaque {
            unsafe {
                ffi::rlDrawRenderBatchActive();
                ffi::rlEnableColorBlend();
            }
        }
        crate::prof::mark("backdrop");
        // Stars, sun, moon and clouds after the solids, so the depth test
        // rejects whatever the planet hides.
        if !crate::prof::skip("sky") {
            gl::additive(|| w.draw_sky(g, &spr, self.cam.pos, t, warp, &sky, sky_d));
        }
        crate::prof::mark("sky");
        if self.hero.is_none() {
            self.player.draw_scarf(&light, t, sky.ink);
        }
        self.draw_strings(sky.ink);
        gl::alpha(|| {
            if crate::prof::skip("alpha") {
                return;
            }
            Sprites::begin(g.sprite);
            self.player.draw_shadow();
            Sprites::end();
            self.fx.draw_dust(&spr, g.sprite);
            w.draw_alpha(g, t, self.cam.pos);
        });
        crate::prof::mark("scarf+dust");
        if !crate::prof::skip("glass") {
            w.draw_glass(g, &self.kit);
        }
        gl::additive(|| {
            if crate::prof::skip("glow") {
                return;
            }
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
#[allow(clippy::too_many_arguments)]
fn draw_backdrop(cam: V3, fwd: V3, right: V3, up: V3, fovy: f32, top: Color, bottom: Color, d: f32) {
    // `d` is well inside the far plane: on a 16-bit depth buffer a backdrop
    // near it rounds to the cleared depth, fails the test and flickers black.
    let h = d * (fovy.to_radians() * 0.5).tan() * 1.1;
    let w = h * (gl::W as f32 / gl::H as f32) * 1.1;
    let c = cam + fwd * d;
    unsafe {
        ffi::rlDisableBackfaceCulling();
        ffi::rlSetTexture(0);
        ffi::rlBegin(ffi::RL_QUADS as i32);
        for (p, col) in [
            (c - right * w + up * h, top),
            (c - right * w - up * h, bottom),
            (c + right * w - up * h, bottom),
            (c + right * w + up * h, top),
        ] {
            ffi::rlColor4ub(col.r, col.g, col.b, 255);
            ffi::rlVertex3f(p.x, p.y, p.z);
        }
        ffi::rlEnd();
        ffi::rlDrawRenderBatchActive();
        ffi::rlEnableBackfaceCulling();
    }
}

fn puff(n: usize, cols: &[Color; 2], speed: f32) -> Burst<'_> {
    Burst {
        n,
        speed: (0.3 * speed, 0.9 * speed),
        size: (0.04, 0.08),
        life: (0.4, 0.8),
        cols,
        glow: false,
        grav: 1.5,
        drag: 3.0,
        aim: 0.55,
    }
}
