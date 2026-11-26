//! Buttons from the keyboard, raylib's gamepads and, on Gizlet OS, the
//! handheld's own evdev devices, merged into one pad each frame.
//!
//! Keyboard: arrows or WASD move; Space, Enter or K is A; Backspace or J is B;
//! I is X; U is Y; Esc or P is Start; Tab is Select. Q/E and R/F turn the
//! camera like the right stick.

use raylib_sys::{self as ffi, GamepadAxis as Axis, GamepadButton as G, KeyboardKey as K};

/// A physical button. Face buttons are named by position, Nintendo style:
/// A right, B bottom, X top, Y left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(all(target_os = "linux", feature = "gizlet-os")), allow(dead_code))]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Select,
    Start,
    Home,
    L3,
    R3,
    VolumeUp,
    VolumeDown,
    Power,
}

pub const BUTTONS: usize = 20;

/// The physical state of the controls this frame.
#[derive(Clone, Default)]
pub struct RawPad {
    #[cfg_attr(not(all(target_os = "linux", feature = "gizlet-os")), allow(dead_code))]
    pub buttons: [bool; BUTTONS],
    /// Left X/Y and right X/Y in -1..1, then the left and right triggers in 0..1.
    pub axes: [f32; 6],
}

impl RawPad {
    pub fn set(&mut self, b: Button) {
        self.buttons[b as usize] = true;
    }

    /// Keeps the strongest deflection reported for this axis.
    pub fn merge_axis(&mut self, slot: usize, value: f32) {
        let axis = &mut self.axes[slot];
        if value.abs() > axis.abs() {
            *axis = value;
        }
    }
}

const KEYS: [(K, Button); 18] = [
    (K::KEY_UP, Button::Up),
    (K::KEY_W, Button::Up),
    (K::KEY_DOWN, Button::Down),
    (K::KEY_S, Button::Down),
    (K::KEY_LEFT, Button::Left),
    (K::KEY_A, Button::Left),
    (K::KEY_RIGHT, Button::Right),
    (K::KEY_D, Button::Right),
    (K::KEY_SPACE, Button::A),
    (K::KEY_ENTER, Button::A),
    (K::KEY_K, Button::A),
    (K::KEY_BACKSPACE, Button::B),
    (K::KEY_J, Button::B),
    (K::KEY_I, Button::X),
    (K::KEY_U, Button::Y),
    (K::KEY_ESCAPE, Button::Start),
    (K::KEY_P, Button::Start),
    (K::KEY_TAB, Button::Select),
];

const PAD: [(G, Button); 12] = [
    (G::GAMEPAD_BUTTON_LEFT_FACE_UP, Button::Up),
    (G::GAMEPAD_BUTTON_LEFT_FACE_DOWN, Button::Down),
    (G::GAMEPAD_BUTTON_LEFT_FACE_LEFT, Button::Left),
    (G::GAMEPAD_BUTTON_LEFT_FACE_RIGHT, Button::Right),
    (G::GAMEPAD_BUTTON_RIGHT_FACE_RIGHT, Button::A),
    (G::GAMEPAD_BUTTON_RIGHT_FACE_DOWN, Button::B),
    (G::GAMEPAD_BUTTON_RIGHT_FACE_UP, Button::X),
    (G::GAMEPAD_BUTTON_RIGHT_FACE_LEFT, Button::Y),
    (G::GAMEPAD_BUTTON_LEFT_TRIGGER_1, Button::L1),
    (G::GAMEPAD_BUTTON_RIGHT_TRIGGER_1, Button::R1),
    (G::GAMEPAD_BUTTON_MIDDLE_LEFT, Button::Select),
    (G::GAMEPAD_BUTTON_MIDDLE_RIGHT, Button::Start),
];

const DIRS: [Button; 4] = [Button::Up, Button::Down, Button::Left, Button::Right];

/// Menu auto-repeat for one direction: how long it has now been held, and
/// whether it repeats this frame. It repeats only while still held, first
/// after 0.38 s and then every 0.11 s.
fn repeat(held: f32, down: bool, dt: f32) -> (f32, bool) {
    const FIRST: f32 = 0.38;
    const EVERY: f32 = 0.11;
    if !down {
        return (0.0, false);
    }
    let now = held + dt;
    let fires = held >= FIRST && ((held - FIRST) / EVERY).floor() != ((now - FIRST) / EVERY).floor();
    (now, fires)
}

/// Applies the walking dead zone and caps speed at full stick deflection.
fn walking_stick(x: f32, y: f32) -> (f32, f32) {
    let magnitude = x.hypot(y);
    if magnitude > 0.22 {
        let scale = magnitude.max(1.0);
        (x / scale, y / scale)
    } else {
        (0.0, 0.0)
    }
}

pub struct Controls {
    cur: [bool; BUTTONS],
    prev: [bool; BUTTONS],
    /// Seconds each direction has been held, for menu auto-repeat.
    held: [f32; 4],
    repeat: [bool; 4],
    stick: (f32, f32),
    look: (f32, f32),
    swap_ab: bool,
    #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
    evdev: crate::evdev::Evdev,
}

impl Controls {
    pub fn new() -> Controls {
        Controls {
            cur: [false; BUTTONS],
            prev: [false; BUTTONS],
            held: [0.0; 4],
            repeat: [false; 4],
            stick: (0.0, 0.0),
            look: (0.0, 0.0),
            swap_ab: std::env::var("GIZLET_SWAP_AB").is_ok_and(|v| v == "1"),
            #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
            evdev: crate::evdev::Evdev::default(),
        }
    }

    #[cfg_attr(not(all(target_os = "linux", feature = "gizlet-os")), allow(unused_variables))]
    pub fn poll(&mut self, dt: f32, time: f64) {
        let mut pad = RawPad::default();
        unsafe {
            for (k, b) in KEYS {
                if ffi::IsKeyDown(k as i32) {
                    pad.set(b);
                }
            }
        }
        #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
        let handheld = {
            self.evdev.poll(time);
            self.evdev.state(&mut pad)
        };
        #[cfg(not(all(target_os = "linux", feature = "gizlet-os")))]
        let handheld = false;
        if !handheld {
            unsafe {
                for g in 0..4 {
                    if !ffi::IsGamepadAvailable(g) {
                        continue;
                    }
                    for (gb, b) in PAD {
                        if ffi::IsGamepadButtonDown(g, gb as i32) {
                            pad.set(b);
                        }
                    }
                    let x = ffi::GetGamepadAxisMovement(g, Axis::GAMEPAD_AXIS_LEFT_X as i32);
                    let y = ffi::GetGamepadAxisMovement(g, Axis::GAMEPAD_AXIS_LEFT_Y as i32);
                    pad.merge_axis(0, x);
                    pad.merge_axis(1, y);
                    let rx = ffi::GetGamepadAxisMovement(g, Axis::GAMEPAD_AXIS_RIGHT_X as i32);
                    let ry = ffi::GetGamepadAxisMovement(g, Axis::GAMEPAD_AXIS_RIGHT_Y as i32);
                    pad.merge_axis(2, rx);
                    pad.merge_axis(3, ry);
                }
            }
        }
        if self.swap_ab {
            pad.buttons.swap(Button::A as usize, Button::B as usize);
        }
        let (x, y) = (pad.axes[0], pad.axes[1]);
        self.stick = walking_stick(x, y);
        // The right stick turns the camera; past the dead zone it ramps up
        // from zero so small pushes give fine control.
        let (rx, ry) = (pad.axes[2], pad.axes[3]);
        let m = rx.hypot(ry);
        self.look = if m > 0.18 {
            let k = ((m - 0.18) / 0.82).min(1.0);
            let k = k * k / m;
            (rx * k, ry * k)
        } else {
            (0.0, 0.0)
        };
        unsafe {
            let key = |k: K| ffi::IsKeyDown(k as i32) as i32 as f32;
            let kx = key(K::KEY_E) - key(K::KEY_Q);
            let ky = key(K::KEY_F) - key(K::KEY_R);
            if kx != 0.0 || ky != 0.0 {
                self.look = (kx, ky);
            }
        }
        // The stick also steers menus.
        if y < -0.6 {
            pad.set(Button::Up);
        }
        if y > 0.6 {
            pad.set(Button::Down);
        }
        if x < -0.6 {
            pad.set(Button::Left);
        }
        if x > 0.6 {
            pad.set(Button::Right);
        }
        self.prev = self.cur;
        self.cur = pad.buttons;
        for (i, d) in DIRS.iter().enumerate() {
            (self.held[i], self.repeat[i]) = repeat(self.held[i], self.cur[*d as usize], dt);
        }
    }

    pub fn down(&self, b: Button) -> bool {
        self.cur[b as usize]
    }

    pub fn pressed(&self, b: Button) -> bool {
        self.cur[b as usize] && !self.prev[b as usize]
    }

    /// Pressed, or repeating while held: for menu directions.
    #[allow(dead_code)]
    pub fn nav(&self, b: Button) -> bool {
        self.pressed(b) || DIRS.iter().position(|d| *d == b).is_some_and(|i| self.repeat[i])
    }

    /// The camera stick: right (+x) and down (+y), each -1..1.
    pub fn look(&self) -> (f32, f32) {
        self.look
    }

    /// The walking direction: the stick when it is pushed, else the D-pad.
    pub fn movement(&self) -> (f32, f32) {
        if self.stick != (0.0, 0.0) {
            return self.stick;
        }
        let mut x = 0.0;
        let mut y = 0.0;
        if self.down(Button::Left) {
            x -= 1.0;
        }
        if self.down(Button::Right) {
            x += 1.0;
        }
        if self.down(Button::Up) {
            y -= 1.0;
        }
        if self.down(Button::Down) {
            y += 1.0;
        }
        let l: f32 = f32::hypot(x, y);
        if l > 0.0 {
            (x / l, y / l)
        } else {
            (0.0, 0.0)
        }
    }

    /// The button label to show for an action, after the A/B swap.
    #[allow(dead_code)]
    pub fn label(&self, b: Button) -> &'static str {
        match (b, self.swap_ab) {
            (Button::A, false) | (Button::B, true) => "A",
            (Button::A, true) | (Button::B, false) => "B",
            (Button::X, _) => "X",
            (Button::Y, _) => "Y",
            (Button::Start, _) => "Start",
            (Button::Select, _) => "Select",
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{repeat, walking_stick, RawPad};

    #[test]
    fn idle_controller_preserves_active_sticks_and_triggers() {
        let active = [-0.7, 0.2, -0.9, 0.6, 0.5, 1.0];
        for active_first in [true, false] {
            let mut pad = RawPad::default();
            let idle = [0.0; 6];
            let controllers = if active_first { [active, idle] } else { [idle, active] };
            for axes in controllers {
                for (slot, value) in axes.into_iter().enumerate() {
                    pad.merge_axis(slot, value);
                }
            }
            assert_eq!(pad.axes, active);
            pad.merge_axis(1, -0.4);
            assert_eq!(pad.axes[1], -0.4);
        }
    }

    #[test]
    fn walking_stick_caps_diagonal_speed() {
        let (x, y) = walking_stick(1.0, -1.0);
        assert!((x.hypot(y) - 1.0).abs() < 1e-6);
        assert_eq!(x, -y);
        assert!(x > 0.0);
        assert_eq!(walking_stick(1.0, 0.0), (1.0, 0.0));
        assert_eq!(walking_stick(0.3, -0.4), (0.3, -0.4));
        assert_eq!(walking_stick(0.1, 0.1), (0.0, 0.0));
    }

    /// Counts repeats over `frames` held frames, then one released frame.
    fn run(frames: usize) -> (usize, bool) {
        let (mut held, mut n) = (0.0, 0);
        for _ in 0..frames {
            let (h, fires) = repeat(held, true, 1.0 / 60.0);
            held = h;
            n += fires as usize;
        }
        (n, repeat(held, false, 1.0 / 60.0).1)
    }

    #[test]
    fn repeats_while_held_only() {
        assert_eq!(run(20).0, 0, "no repeat before 0.38 s");
        let (n, on_release) = run(60);
        assert!((5..=6).contains(&n), "about one repeat per 0.11 s after 0.38 s, got {n}");
        assert!(!on_release, "releasing a held direction must not step the menu");
    }
}
