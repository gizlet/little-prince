//! The Little Prince · 小王子: a 3D planet-walk game for Gizlet OS
//! handhelds in Rust + raylib. The hero walks B612, the lamplighter's
//! planet and the desert where the fox is tamed, and migrating birds carry
//! it between them; the HUD shows FPS, CPU load and free memory.
//!
//! The planets follow Antoine de Saint-Exupéry's book. The hero is KayKit
//! Adventurers' hooded Rogue by Kay Lousberg (CC0); every other model and
//! effect here is written from scratch and made in code.
//!
//! Environment:
//! - `GIZLET_LANG=en`: start in English (Simplified Chinese otherwise).
//! - `GIZLET_SWAP_AB=1`: swap the A and B buttons.
//! - `LITTLEPRINCE_WINDOW=WxH`: the desktop window size (default 1280x960).
//! - `LITTLEPRINCE_AUTOPLAY=1`: start autoplay at once.
//! - `LITTLEPRINCE_MUTE=1`: no sound.
//! - `LITTLEPRINCE_CAPTURE=FILE.png` with `LITTLEPRINCE_SCENE` (`title`, `icon`,
//!   `b612`, `lamp`, `desert`, `fly`, `journeys`, `pause`, `settings`,
//!   `statistics`) and `LITTLEPRINCE_AT` (seconds of autoplay first):
//!   render one frame to a PNG and exit.
//! - `LITTLEPRINCE_SECONDS=N`: quit after N seconds (for scripted runs).
//! - `LITTLEPRINCE_SCRIPT=FILE`: replay the button presses in FILE and check
//!   the game's state as it goes; exits with status 1 if a check fails (see
//!   `script`).
//! - `GIZLET_SAVE_DIR`: where journeys and settings are kept (`./save`
//!   otherwise); captures save nothing.

mod app;
#[cfg(all(target_os = "linux", feature = "gizlet-os"))]
mod evdev;
mod fx;
mod gl;
mod hero;
mod hud;
mod input;
mod math;
mod menu;
mod mesh;
mod player;
mod prof;
mod save;
mod script;
mod sound;
mod stats;
mod text;
mod world;

use app::{App, Mode};
use raylib_sys::{self as ffi, Camera2D, ConfigFlags as F, Rectangle, Vector2};
use std::ffi::CString;

/// Without a window system: raylib's DRM platform on the whole panel.
const KMS: bool = cfg!(feature = "gizlet-os");

fn env_size(name: &str) -> Option<(i32, i32)> {
    let v = std::env::var(name).ok()?;
    let (w, h) = v.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

fn env_on(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1")
}

/// Where the 640×480 canvas goes on the screen, letterboxed.
fn view() -> (Rectangle, f32) {
    let (sw, sh) = unsafe { (ffi::GetRenderWidth() as f32, ffi::GetRenderHeight() as f32) };
    let k = (sw / gl::W as f32).min(sh / gl::H as f32);
    let (w, h) = (gl::W as f32 * k, gl::H as f32 * k);
    (Rectangle { x: ((sw - w) / 2.0).floor(), y: ((sh - h) / 2.0).floor(), width: w, height: h }, k)
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        let version = option_env!("GIZLET_APP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
        println!("littleprince {version}");
        return;
    }
    let capture = std::env::var("LITTLEPRINCE_CAPTURE").ok();
    let (w, h) = if KMS { (0, 0) } else { env_size("LITTLEPRINCE_WINDOW").unwrap_or((1280, 960)) };
    let mut flags = F::FLAG_VSYNC_HINT as u32;
    if !KMS {
        flags |= F::FLAG_WINDOW_RESIZABLE as u32;
    }
    if capture.is_some() {
        flags |= F::FLAG_WINDOW_HIDDEN as u32;
    }
    unsafe {
        ffi::SetTraceLogLevel(if KMS { ffi::TraceLogLevel::LOG_INFO } else { ffi::TraceLogLevel::LOG_WARNING } as i32);
        ffi::SetConfigFlags(flags);
        ffi::InitWindow(w, h, c"The Little Prince · 小王子".as_ptr());
        // raylib reports the window ready even when the DRM platform failed
        // (another program holding the display); then no GL context exists.
        if !ffi::IsWindowReady() || ffi::rlGetShaderIdDefault() == 0 {
            eprintln!("littleprince: the display could not be opened (is another program using it?)");
            std::process::exit(1);
        }
        // raylib's one batch buffer is rewritten at every flush; on Panfrost
        // rewriting a buffer the frame still uses stalls the CPU for
        // milliseconds. Many buffers in rotation keep each flush on a fresh one.
        let buffers = std::env::var("LITTLEPRINCE_BATCH").ok().and_then(|v| v.parse().ok()).unwrap_or(48);
        if buffers > 1 {
            let batch = Box::leak(Box::new(ffi::rlLoadRenderBatch(buffers, 1024)));
            ffi::rlSetRenderBatchActive(batch);
        }
        ffi::SetExitKey(0);
        // 0 leaves the pace to the display's vsync (78 Hz on the R36S panel).
        ffi::SetTargetFPS(std::env::var("LITTLEPRINCE_FPS").ok().and_then(|v| v.parse().ok()).unwrap_or(60));
        ffi::rlSetClipPlanes(0.05, 150.0);
        if !KMS {
            ffi::SetWindowMinSize(320, 240);
        }
    }
    // Sound is a bonus: a device without audio still plays.
    let audio = capture.is_none()
        && !env_on("LITTLEPRINCE_MUTE")
        && unsafe {
            ffi::InitAudioDevice();
            ffi::IsAudioDeviceReady()
        };
    let lang =
        if std::env::var("GIZLET_LANG").is_ok_and(|v| v.starts_with("en")) { text::Lang::En } else { text::Lang::Zh };
    let zoom = view().1.max(1.0);
    let mut controls = input::Controls::new();
    let mut app = App::new(lang, zoom, audio, save::Store::from_env(capture.is_none()));
    if env_on("LITTLEPRINCE_AUTOPLAY") {
        app.autoplay = true;
        app.start_play(0);
    }
    if capture.is_some() {
        setup_capture(&mut app, &controls);
    }

    if let Ok(v) = std::env::var("LITTLEPRINCE_PROFILE") {
        prof::enable(v != "2", v == "1");
    }
    let start = unsafe { ffi::GetTime() };
    let quit_after: Option<f64> = std::env::var("LITTLEPRINCE_SECONDS").ok().and_then(|v| v.parse().ok());
    let mut replay = std::env::var("LITTLEPRINCE_SCRIPT").ok().map(|path| {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("littleprince: cannot read {path}: {e}"));
        script::Player::new(script::parse(&text).unwrap_or_else(|e| panic!("littleprince: {path}: {e}")))
    });
    let mut steps = 0u32;
    while !unsafe { ffi::WindowShouldClose() } && !app.quit {
        let frame = prof::start_frame();
        let fixed = capture.is_some() || replay.is_some();
        let dt = if fixed { 1.0 / 60.0 } else { unsafe { ffi::GetFrameTime() }.min(0.1) };
        if let Some(r) = &mut replay {
            controls.scripted = r.step(steps as f32 / 60.0, &|name| app.probe(name));
            steps += 1;
            if r.done {
                break;
            }
        }
        let now = unsafe { ffi::GetTime() } - start;
        if quit_after.is_some_and(|s| now >= s) {
            break;
        }
        controls.poll(dt, now);
        if capture.is_none() {
            app.update(dt, &controls);
        }
        prof::mark("update");
        let (dest, k) = view();
        let direct = app.gl.direct(app.fx_level);
        if !direct {
            app.render(false);
        }
        if app.hud {
            hud::prepare(&app, k);
        }
        prof::mark("hud-prepare");
        unsafe {
            ffi::BeginDrawing();
            ffi::ClearBackground(gl::rgb(0x000000));
        }
        if direct {
            app.render(true);
        } else {
            app.present(dest);
        }
        prof::mark("post");
        unsafe {
            ffi::BeginMode2D(Camera2D {
                offset: Vector2 { x: dest.x, y: dest.y },
                target: Vector2 { x: 0.0, y: 0.0 },
                rotation: 0.0,
                zoom: k,
            });
        }
        if app.hud {
            hud::draw(&app);
        }
        unsafe { ffi::EndMode2D() };
        prof::mark("hud");
        if let Some(path) = &capture {
            unsafe {
                ffi::rlDrawRenderBatchActive();
                let image = ffi::LoadImageFromScreen();
                if let Ok(c) = CString::new(path.as_str()) {
                    ffi::ExportImage(image, c.as_ptr());
                }
                ffi::UnloadImage(image);
                ffi::EndDrawing();
            }
            break;
        }
        if let Some(frame) = &frame {
            frame.before_swap();
        }
        unsafe { ffi::EndDrawing() };
        prof::mark("swap");
        if let Some(frame) = frame {
            frame.finish(|tris| {
                format!("{:?} fx{} parts{} tris{}k", app.world().kind, app.fx_level, app.fx.count(), tris / 1000)
            });
        }
    }
    app.save_now();
    unsafe { ffi::CloseWindow() };
    if let Some(r) = replay {
        eprintln!("script: {} checks, {} failed", r.checks, r.failures);
        // Stopping early, by closing the window or LITTLEPRINCE_SECONDS,
        // leaves checks unplayed: that is a failure too.
        if !r.done {
            eprintln!("script: FAIL the game stopped before the script ended");
        }
        if r.failures > 0 || !r.done {
            std::process::exit(1);
        }
    }
}

/// Plays the scene a capture asks for, at a fixed 60 steps a second.
fn setup_capture(app: &mut App, c: &input::Controls) {
    let scene = std::env::var("LITTLEPRINCE_SCENE").unwrap_or_else(|_| "b612".into());
    let at: f32 = std::env::var("LITTLEPRINCE_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(8.0);
    let step = |app: &mut App, secs: f32| {
        for _ in 0..(secs * 60.0) as usize {
            app.update(1.0 / 60.0, c);
        }
    };
    match scene.as_str() {
        "title" => step(app, at.min(6.0)),
        "icon" => {
            app.hud = false;
            step(app, 3.0);
        }
        "model" => {
            app.hud = false;
            app.closeup = true;
            if let Some(h) = &mut app.hero {
                let d = h.check_against_raylib();
                eprintln!("hero {}: bone matrices differ from raylib by at most {d:.6}", h.name);
            }
            app.start_play(0);
            step(app, at);
        }
        "journeys" => {
            // Two journeys under way and an empty slot.
            let mut j = save::Journey::start();
            j.planet = 1;
            j.visited = [true, true, false];
            j.scores = [7, 12, 0];
            j.stats = save::Stats { time: 1834.0, distance: 2210.0, jumps: 96, sunsets: 31, sprouts: 23, flights: 4 };
            app.journeys[0] = save::Slot::Journey(j.clone());
            j.planet = 2;
            j.visited = [true, true, true];
            j.stats.time = 4210.0;
            app.journeys[2] = save::Slot::Journey(j);
            step(app, at.min(4.0));
            app.menu = Some(menu::Menu::open(menu::Page::Journeys, 0));
        }
        "pause" | "settings" | "statistics" => {
            app.autoplay = true;
            app.start_play(0);
            step(app, at);
            app.autoplay = false;
            app.journey.stats =
                save::Stats { time: 1834.0, distance: 2210.0, jumps: 96, sunsets: 31, sprouts: 23, flights: 4 };
            app.journey.visited = [true, true, false];
            let page = match scene.as_str() {
                "settings" => menu::Page::Settings,
                "statistics" => menu::Page::Statistics,
                _ => menu::Page::Pause,
            };
            app.menu = Some(menu::Menu::open(page, 0));
        }
        "b612" | "lamp" | "desert" | "fly" => {
            let idx = match scene.as_str() {
                "lamp" => 1,
                "desert" => 2,
                _ => 0,
            };
            app.autoplay = true;
            app.start_play(idx);
            step(app, at);
            if scene == "fly" {
                app.begin_flight();
                step(app, 1.5);
            }
            assert!(app.mode != Mode::Title);
        }
        other => eprintln!("littleprince: unknown scene {other}"),
    }
}
