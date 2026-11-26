//! A frame profiler for `LITTLEPRINCE_PROFILE=1`: `mark` waits for the GPU
//! (glFinish) and charges the time since the last mark to a stage; every
//! three seconds the averages go to stderr. Off by default.

use raylib_sys as ffi;
use std::cell::RefCell;

#[derive(Default)]
struct Prof {
    on: bool,
    split: bool,
    /// Wait for the GPU at each mark (`1`); `3` times the CPU side only.
    wait: bool,
    last: f64,
    stages: Vec<(&'static str, f64)>,
    frames: u32,
    since: f64,
    finish: Option<unsafe extern "C" fn()>,
    tris: Vec<(&'static str, u64)>,
    frame_tris: u64,
    draws: u64,
}

thread_local! {
    static P: RefCell<Prof> = RefCell::new(Prof::default());
}

pub fn enable(split: bool, wait: bool) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        p.on = true;
        p.split = split;
        p.wait = wait;
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

/// Records one mesh submission, optionally grouped by terrain LOD.
pub fn mesh(tris: u64, lod: Option<&'static str>) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        if !p.on {
            return;
        }
        p.frame_tris += tris;
        p.draws += 1;
        if let Some(name) = lod {
            match p.tris.iter_mut().find(|s| s.0 == name) {
                Some(s) => s.1 += tris,
                None => p.tris.push((name, tris)),
            }
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

/// Adds `dt` seconds to stage `name` without waiting for the GPU.
fn add(name: &'static str, dt: f64) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        if !p.on {
            return;
        }
        match p.stages.iter_mut().find(|s| s.0 == name) {
            Some(s) => s.1 += dt,
            None => p.stages.push((name, dt)),
        }
    });
}

/// Ends the stage `name` here.
pub fn mark(name: &'static str) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        if !p.on || !p.split {
            return;
        }
        if p.wait {
            unsafe {
                ffi::rlDrawRenderBatchActive();
                if let Some(f) = p.finish {
                    f();
                }
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

/// Seconds of CPU time the calling thread has used.
fn thread_cpu() -> f64 {
    #[cfg(unix)]
    unsafe {
        let mut ts: libc::timespec = std::mem::zeroed();
        libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts);
        ts.tv_sec as f64 + ts.tv_nsec as f64 * 1e-9
    }
    #[cfg(not(unix))]
    0.0
}

pub struct FrameTimer {
    wall_start: f64,
    cpu_start: f64,
}

pub fn start_frame() -> Option<FrameTimer> {
    P.with(|p| p.borrow().on.then(|| FrameTimer { wall_start: unsafe { ffi::GetTime() }, cpu_start: thread_cpu() }))
}

impl FrameTimer {
    pub fn before_swap(&self) {
        add("cpu", unsafe { ffi::GetTime() } - self.wall_start);
    }

    /// Prints the averages every three seconds.
    pub fn finish(self, describe: impl FnOnce(u64) -> String) {
        add("thread-cpu", thread_cpu() - self.cpu_start);
        P.with(|p| {
            let mut p = p.borrow_mut();
            let tris = std::mem::take(&mut p.frame_tris);
            p.frames += 1;
            let now = unsafe { ffi::GetTime() };
            if now - p.since >= 3.0 {
                let n = p.frames as f64;
                let total = (now - p.since) / n * 1000.0;
                let parts: Vec<String> = p.stages.iter().map(|(s, t)| format!("{s} {:.1}", t / n * 1000.0)).collect();
                let lod: Vec<String> = p.tris.iter().map(|(s, c)| format!("{s} {}k", c / p.frames as u64 / 1000)).collect();
                let draws = p.draws / p.frames as u64;
                // Keep the existing report format consumed by profiling scripts.
                eprintln!("prof: {:.1} fps {:.1} ms | {} | {} | {} draws {draws}k", 1000.0 / total, total, parts.join(" · "), describe(tris), lod.join(" "));
                p.tris.clear();
                p.stages.clear();
                p.draws = 0;
                p.frames = 0;
                p.since = now;
            }
        });
    }
}
