//! Sound effects synthesized at start-up, so the app ships no audio files:
//! bell chimes on a pentatonic scale, a blossom pop, a warp whoosh, a landing
//! thump, splashes and soft footsteps. A device without audio stays silent.

use crate::math::Rng;
use raylib_sys::{self as ffi, Sound, Wave};
use std::f32::consts::TAU;

const RATE: u32 = 22050;
const NOTES: [f32; 8] = [523.25, 587.33, 659.25, 783.99, 880.0, 1046.5, 1174.66, 1318.51];
const VOICES: usize = 3;

#[derive(Clone, Copy)]
pub enum Sfx {
    Chime(usize),
    Pop,
    Whoosh,
    Land,
    Splash,
    Step,
}

pub struct Sounds {
    on: bool,
    chimes: Vec<Vec<Sound>>,
    pop: Vec<Sound>,
    whoosh: Vec<Sound>,
    land: Vec<Sound>,
    splash: Vec<Sound>,
    step: Vec<Sound>,
    next: usize,
}

fn load(samples: &[f32]) -> Vec<Sound> {
    let mut pcm: Vec<i16> = samples.iter().map(|s| (s.clamp(-1.0, 1.0) * 32000.0) as i16).collect();
    let wave = Wave { frameCount: pcm.len() as u32, sampleRate: RATE, sampleSize: 16, channels: 1, data: pcm.as_mut_ptr() as _ };
    // raylib copies the samples into its own buffer.
    let first = unsafe { ffi::LoadSoundFromWave(wave) };
    let mut v = vec![first];
    for _ in 1..VOICES {
        v.push(unsafe { ffi::LoadSoundAlias(first) });
    }
    v
}

fn len(seconds: f32) -> usize {
    (seconds * RATE as f32) as usize
}

fn chime(f: f32) -> Vec<f32> {
    (0..len(1.6))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let attack = (t * 300.0).min(1.0);
            let a = (TAU * f * t).sin() * 0.6 * (-t * 2.8).exp();
            let b = (TAU * f * 2.01 * t).sin() * 0.25 * (-t * 5.0).exp();
            let c = (TAU * f * 3.98 * t).sin() * 0.12 * (-t * 9.0).exp();
            (a + b + c) * attack * 0.45
        })
        .collect()
}

fn pop() -> Vec<f32> {
    let mut ph = 0.0f32;
    (0..len(0.3))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = 360.0 + 820.0 * (1.0 - (-t * 22.0).exp());
            ph += TAU * f / RATE as f32;
            let body = ph.sin() * (-t * 14.0).exp();
            let sparkle = (TAU * 2400.0 * t).sin() * 0.18 * (-t * 28.0).exp();
            (body + sparkle) * (t * 500.0).min(1.0) * 0.5
        })
        .collect()
}

fn noise(rng: &mut Rng, seconds: f32, cut: impl Fn(f32) -> f32, env: impl Fn(f32) -> f32) -> Vec<f32> {
    let mut y = 0.0f32;
    (0..len(seconds))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            y += (rng.range(-1.0, 1.0) - y) * cut(t);
            y * env(t)
        })
        .collect()
}

fn thump() -> Vec<f32> {
    let mut ph = 0.0f32;
    let mut rng = Rng::new(5);
    (0..len(0.45))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            ph += TAU * (95.0 - 50.0 * t) / RATE as f32;
            ph.sin() * (-t * 9.0).exp() * 0.8 + rng.range(-1.0, 1.0) * 0.15 * (-t * 30.0).exp()
        })
        .collect()
}

impl Sounds {
    pub fn new(on: bool) -> Sounds {
        let mut s =
            Sounds { on, chimes: vec![], pop: vec![], whoosh: vec![], land: vec![], splash: vec![], step: vec![], next: 0 };
        if !on {
            return s;
        }
        let mut rng = Rng::new(17);
        s.chimes = NOTES.iter().map(|&f| load(&chime(f))).collect();
        s.pop = load(&pop());
        s.whoosh = load(&noise(&mut rng, 1.8, |t| 0.03 + 0.3 * (t / 1.1).min(1.0), |t| (t / 1.8 * std::f32::consts::PI).sin().powi(2) * 0.7));
        s.land = load(&thump());
        s.splash = load(&noise(&mut rng, 0.4, |_| 0.55, |t| (-t * 9.0).exp() * (t * 200.0).min(1.0) * 0.45));
        s.step = load(&noise(&mut rng, 0.07, |_| 0.25, |t| (-t * 70.0).exp() * 0.5));
        unsafe { ffi::SetMasterVolume(0.8) };
        s
    }

    pub fn play(&mut self, sfx: Sfx, pitch: f32, volume: f32) {
        if !self.on {
            return;
        }
        let set = match sfx {
            Sfx::Chime(i) => &self.chimes[i % NOTES.len()],
            Sfx::Pop => &self.pop,
            Sfx::Whoosh => &self.whoosh,
            Sfx::Land => &self.land,
            Sfx::Splash => &self.splash,
            Sfx::Step => &self.step,
        };
        self.next = (self.next + 1) % VOICES;
        let snd = set[self.next];
        unsafe {
            ffi::SetSoundPitch(snd, pitch);
            ffi::SetSoundVolume(snd, volume);
            ffi::PlaySound(snd);
        }
    }
}
