//! A frame profiler for `LITTLEPRINCE_PROFILE=1`: `mark` waits for the GPU
//! (glFinish) and charges the time since the last mark to a stage; every
//! three seconds the averages go to stderr. Off by default, at no cost.

use raylib_sys as ffi;
use std::cell::RefCell;

#[derive(Default)]
struct Prof {
    on: bool,
    split: bool,
    last: f64,
    stages: Vec<(&'static str, f64)>,
    frames: u32,
    since: f64,
    finish: Option<unsafe extern "C" fn()>,
    tris: Vec<(&'static str, u64)>,
}

thread_local! {
    static P: RefCell<Prof> = RefCell::new(Prof::default());
}

pub fn enable(split: bool) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        p.on = true;
        p.split = split;
        #[cfg(unix)]
        unsafe {
            let f = libc::dlsym(libc::RTLD_DEFAULT, c"glFinish".as_ptr());
            if !f.is_null() {
                p.finish = Some(std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn()>(f));
            }
        }
        p.last = unsafe { ffi::GetTime() };
        p.since = p.last;
    });
}

pub fn on() -> bool {
    P.with(|p| p.borrow().on)
}

/// Counts `n` triangles under `name` (shown with `LITTLEPRINCE_PROFILE`).
pub fn tris(name: &'static str, n: u64) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        if !p.on {
            return;
        }
        match p.tris.iter_mut().find(|s| s.0 == name) {
            Some(s) => s.1 += n,
            None => p.tris.push((name, n)),
        }
    });
}

/// Whether `LITTLEPRINCE_SKIP` (a comma list) names `part`: for measuring.
pub fn skip(part: &str) -> bool {
    thread_local! {
        static S: Vec<String> = std::env::var("LITTLEPRINCE_SKIP").unwrap_or_default().split(',').map(str::to_string).collect();
    }
    S.with(|s| s.iter().any(|x| x == part))
}

/// Ends the stage `name` here.
pub fn mark(name: &'static str) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        if !p.on || !p.split {
            return;
        }
        unsafe {
            ffi::rlDrawRenderBatchActive();
            if let Some(f) = p.finish {
                f();
            }
        }
        let now = unsafe { ffi::GetTime() };
        let dt = now - p.last;
        p.last = now;
        match p.stages.iter_mut().find(|s| s.0 == name) {
            Some(s) => s.1 += dt,
            None => p.stages.push((name, dt)),
        }
    });
}

/// Ends a frame; prints the averages every three seconds.
pub fn frame(extra: &str) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        if !p.on {
            return;
        }
        p.frames += 1;
        let now = unsafe { ffi::GetTime() };
        if now - p.since >= 3.0 {
            let n = p.frames as f64;
            let total = (now - p.since) / n * 1000.0;
            let parts: Vec<String> = p.stages.iter().map(|(s, t)| format!("{s} {:.1}", t / n * 1000.0)).collect();
            let t: Vec<String> = p.tris.iter().map(|(s, c)| format!("{s} {}k", c / p.frames as u64 / 1000)).collect();
            eprintln!("prof: {:.1} fps {:.1} ms | {} | {extra} | {}", 1000.0 / total, total, parts.join(" · "), t.join(" "));
            p.tris.clear();
            p.stages.clear();
            p.frames = 0;
            p.since = now;
        }
    });
}
