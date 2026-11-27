//! The 2D layer over the scene, in 640×480 canvas coordinates: the title
//! with the source credits, planet banners, the score, the prompt bar and the
//! performance panel (FPS with a frame-time graph, CPU, memory, temperature).

use crate::app::{App, Mode};
use crate::gl::{lighten, rgb, rgba};
use crate::math::{clamp01, smooth};
use crate::menu::{Menu, Page, PauseItem, Setting, PAUSE, SETTINGS};
use crate::text::{s, Align, Lang, Weight, T};
use raylib_sys::{self as ffi, Camera2D, Color, Rectangle, RenderTexture2D, Vector2};
use std::cell::RefCell;

const CREAM: Color = rgb(0xFFF6E6);
const SOFT: Color = rgb(0xE6E4F2);
const LINK: Color = rgb(0xA8DCFF);
const GOLD: Color = rgb(0xF6C744);
const PANEL: Color = rgb(0x1C1830);

fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
    let rn = (r * 2.0 / w.min(h)).min(1.0);
    unsafe { ffi::DrawRectangleRounded(Rectangle { x, y, width: w, height: h }, rn, 8, c) };
}

fn ease_back(t: f32) -> f32 {
    let t = clamp01(t);
    let (c1, c3) = (1.70158, 2.70158);
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// The slow-changing parts of the HUD (the prompt bar and the stats panel),
/// drawn into a texture four times a second instead of every frame: each
/// text and shape switch flushes raylib's batch, which costs milliseconds on
/// the handheld's GPU driver.
struct Cache {
    rt: RenderTexture2D,
    key: (u8, bool, bool, bool),
    at: f32,
}

thread_local! {
    static CACHE: RefCell<Option<Cache>> = const { RefCell::new(None) };
}

/// Refreshes the cached layer; call outside any drawing mode, `k` being the
/// canvas zoom.
pub fn prepare(app: &App, k: f32) {
    if crate::prof::skip("hudcache") {
        return;
    }
    let (w, h) = ((640.0 * k).round() as i32, (480.0 * k).round() as i32);
    let key = (app.fx_level, app.show_stats, app.lang == Lang::Zh, app.mode == Mode::Title);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.as_ref().is_some_and(|c| c.rt.texture.width != w || c.rt.texture.height != h) {
            if let Some(old) = c.take() {
                unsafe { ffi::UnloadRenderTexture(old.rt) };
            }
        }
        let cache = c.get_or_insert_with(|| Cache { rt: unsafe { ffi::LoadRenderTexture(w, h) }, key, at: -1.0 });
        if cache.key == key && app.t - cache.at < 0.25 && cache.at >= 0.0 {
            return;
        }
        cache.key = key;
        cache.at = app.t;
        unsafe {
            ffi::BeginTextureMode(cache.rt);
            ffi::ClearBackground(Color { r: 0, g: 0, b: 0, a: 0 });
            ffi::BeginMode2D(Camera2D {
                offset: Vector2 { x: 0.0, y: 0.0 },
                target: Vector2 { x: 0.0, y: 0.0 },
                rotation: 0.0,
                zoom: k,
            });
            // Keep the texture premultiplied, so it composites like the live HUD.
            ffi::rlSetBlendFactorsSeparate(0x0302, 0x0303, 1, 0x0303, 0x8006, 0x8006);
            ffi::BeginBlendMode(ffi::BlendMode::BLEND_CUSTOM_SEPARATE as i32);
        }
        if app.mode != Mode::Title {
            bar(app);
        }
        if app.show_stats {
            stats(app);
        }
        unsafe {
            ffi::EndBlendMode();
            ffi::EndMode2D();
            ffi::EndTextureMode();
        }
    });
}

fn blit_cache() {
    CACHE.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            let t = c.rt.texture;
            // Only the parts that hold something, not the whole screen: blending
            // a full-screen quad costs the handheld's GPU several milliseconds.
            let k = t.width as f32 / 640.0;
            let mut parts = vec![(0.0f32, 426.0f32, 640.0f32, 54.0f32)];
            if c.key.1 {
                parts.push((4.0, 4.0, 230.0, 108.0));
            }
            unsafe {
                ffi::BeginBlendMode(ffi::BlendMode::BLEND_ALPHA_PREMULTIPLY as i32);
                for (x, y, w, h) in parts {
                    // The texture is upside down: rows are counted from the bottom.
                    let src = Rectangle { x: x * k, y: (480.0 - y - h) * k, width: w * k, height: -h * k };
                    ffi::DrawTexturePro(
                        t,
                        src,
                        Rectangle { x, y, width: w, height: h },
                        Vector2 { x: 0.0, y: 0.0 },
                        0.0,
                        rgb(0xFFFFFF),
                    );
                }
                ffi::EndBlendMode();
            }
        }
    });
}

pub fn draw(app: &App) {
    if !crate::prof::skip("hudlive") {
        match app.mode {
            Mode::Title => title(app),
            _ => play(app),
        }
    }
    crate::prof::mark("hud-live");
    if !crate::prof::skip("hudcache") {
        blit_cache();
    }
    if let Some(m) = &app.menu {
        menu(app, m);
    }
    if app.quit_hold > 0.15 {
        let f = &app.fonts;
        let k = clamp01(app.quit_hold / 2.0);
        rrect(220.0, 214.0, 200.0, 52.0, 10.0, rgba(PANEL, 0.85));
        f.text(s(T::HoldToQuit, app.lang), 320.0, 224.0, 13.0, CREAM, Align::Center);
        rrect(236.0, 248.0, 168.0, 6.0, 3.0, rgba(rgb(0xFFFFFF), 0.2));
        rrect(236.0, 248.0, 168.0 * k, 6.0, 3.0, rgb(0xFF8A7A));
    }
}

fn title(app: &App) {
    let f = &app.fonts;
    let l = app.lang;
    let t = app.mode_t;
    // The title drops in letter by letter and keeps floating; a small star
    // circles it.
    let name = s(T::Title, l);
    let mut size = 56.0;
    let gap = if l == Lang::Zh { 12.0 } else { 1.0 };
    let chars: Vec<String> = name.chars().map(|c| c.to_string()).collect();
    let mut widths: Vec<f32> = chars.iter().map(|c| f.measure(c, size, Weight::Display)).collect();
    let mut total: f32 = widths.iter().sum::<f32>() + gap * (chars.len() as f32 - 1.0);
    if total > 380.0 {
        size *= 380.0 / total;
        widths = chars.iter().map(|c| f.measure(c, size, Weight::Display)).collect();
        total = widths.iter().sum::<f32>() + gap * (chars.len() as f32 - 1.0);
    }
    let y0 = 34.0 + (56.0 - size) * 0.5;
    let mut x = 320.0 - total / 2.0;
    for (i, (c, w)) in chars.iter().zip(&widths).enumerate() {
        let k = clamp01((t - 0.2 - i as f32 * 0.09) / 0.7);
        let drop = (1.0 - ease_back(k)) * -36.0;
        let float = (app.t * 1.6 + i as f32 * 0.7).sin() * 3.0;
        let col = lighten(CREAM, 0.15 * ((app.t * 2.0 + i as f32).sin() * 0.5 + 0.5));
        f.shadowed(c, x, y0 + drop + float, size, rgba(col, smooth(k * 2.0)), Align::Left, Weight::Display);
        x += w + gap;
    }
    let a = smooth((t - 0.6) / 0.6);
    let orbit = app.t * 1.3;
    let star = Vector2 { x: 320.0 + orbit.cos() * (total / 2.0 + 26.0), y: y0 + 30.0 + orbit.sin() * 26.0 };
    unsafe {
        ffi::DrawCircleGradient(star, 12.0, rgba(GOLD, 0.5 * a), rgba(GOLD, 0.0));
        ffi::DrawPoly(star, 5, 4.0, orbit.to_degrees(), rgba(rgb(0xFFF4C0), a));
    }
    let other = if l == Lang::Zh { Lang::En } else { Lang::Zh };
    let a = smooth((t - 0.9) / 0.6);
    f.shadowed(s(T::Title, other), 320.0, 104.0, 16.0, rgba(SOFT, a * 0.85), Align::Center, Weight::Text);
    f.shadowed(s(T::Subtitle, l), 320.0, 128.0, 16.0, rgba(CREAM, a), Align::Center, Weight::Text);

    if t > 1.4 {
        let pulse = 0.6 + 0.4 * (app.t * 3.2).sin();
        let label = s(T::PressStart, l);
        let w = f.measure(label, 16.0, Weight::Text) + 36.0;
        rrect(320.0 - w / 2.0, 336.0, w, 30.0, 15.0, rgba(PANEL, 0.35 * pulse + 0.2));
        f.text(label, 320.0, 341.0, 16.0, rgba(CREAM, pulse), Align::Center);
    }

    // Where this comes from stays on the title screen.
    let a = smooth((t - 0.5) / 0.8);
    rrect(24.0, 380.0, 592.0, 90.0, 12.0, rgba(PANEL, 0.55 * a));
    f.shadowed(s(T::Credit, l), 320.0, 387.0, 13.0, rgba(CREAM, a), Align::Center, Weight::Text);
    f.shadowed(s(T::CreditUrl, l), 320.0, 405.0, 13.0, rgba(LINK, a), Align::Center, Weight::Text);
    f.shadowed(s(T::Book, l), 320.0, 426.0, 11.0, rgba(SOFT, a * 0.9), Align::Center, Weight::Text);
    f.shadowed(s(T::TestNote, l), 320.0, 444.0, 11.0, rgba(SOFT, a * 0.75), Align::Center, Weight::Text);
    f.shadowed(
        &format!("Start · {}", s(T::Settings, l)),
        632.0,
        10.0,
        11.0,
        rgba(SOFT, 0.7),
        Align::Right,
        Weight::Text,
    );
}

fn play(app: &App) {
    let f = &app.fonts;
    let l = app.lang;
    let w = app.world();
    let (name, hint, label) = w.name();

    let bt = app.planet_t;
    if app.mode == Mode::Play && bt < 5.5 {
        let k = ease_back(bt / 0.6);
        let out = 1.0 - smooth((bt - 4.2) / 0.9);
        let a = smooth(bt / 0.35) * out;
        let y = 116.0 + (1.0 - k) * -24.0;
        f.shadowed(s(name, l), 320.0, y, 28.0, rgba(CREAM, a), Align::Center, Weight::Display);
        let ha = smooth((bt - 0.4) / 0.5) * out;
        f.shadowed(s(hint, l), 320.0, y + 38.0, 13.0, rgba(SOFT, ha), Align::Center, Weight::Text);
    }

    // The score, popping when it grows.
    let text = if w.total > 0 {
        format!("{} {}/{}", s(label, l), w.score, w.total)
    } else {
        format!("{} {}", s(label, l), w.score)
    };
    let pop = w.pop * w.pop;
    let size = 16.0 * (1.0 + 0.3 * pop);
    let tw = f.measure(&text, size, Weight::Text);
    let pw = tw + 34.0;
    let h = 26.0 * (1.0 + 0.2 * pop);
    rrect(632.0 - pw, 10.0, pw, h, h / 2.0, rgba(PANEL, 0.55));
    unsafe {
        ffi::DrawPoly(Vector2 { x: 632.0 - pw + 14.0, y: 10.0 + h / 2.0 }, 5, 6.0 + pop * 3.0, app.t * 40.0, GOLD);
    }
    f.text(&text, 632.0 - 10.0, 10.0 + (h - size * 1.2) / 2.0, size, CREAM, Align::Right);

    if app.autoplay {
        let a = 0.55 + 0.45 * (app.t * 2.5).sin();
        f.shadowed(s(T::Autoplay, l), 632.0, 44.0, 11.0, rgba(CREAM, a), Align::Right, Weight::Text);
    }
    if app.mode == Mode::Fly {
        let a = smooth(app.mode_t / 0.5) * (1.0 - smooth((app.mode_t - 3.2) / 0.6));
        f.shadowed(s(T::Warping, l), 320.0, 404.0, 16.0, rgba(CREAM, a), Align::Center, Weight::Text);
    }
}

/// The prompt bar and the credit: they change only with the language and effects level.
fn bar(app: &App) {
    let f = &app.fonts;
    let l = app.lang;
    // The prompt bar.
    let fx = [T::FxLow, T::FxMid, T::FxHigh][app.fx_level as usize];
    let effects = format!("{} {}", s(T::Effects, l), s(fx, l));
    let items: [(&str, &str); 6] = [
        ("A", s(T::Jump, l)),
        ("X", s(T::Travel, l)),
        (s(T::RStick, l), s(T::Camera, l)),
        ("Y", &effects),
        ("Select", s(T::Statistics, l)),
        ("Start", s(T::Menu, l)),
    ];
    let size = 11.0;
    let widths: Vec<(f32, f32)> = items
        .iter()
        .map(|(b, a)| (f.measure(b, size, Weight::Text) + 10.0, f.measure(a, size, Weight::Text)))
        .collect();
    let total: f32 = widths.iter().map(|(b, a)| b + 4.0 + a).sum::<f32>() + 12.0 * (items.len() as f32 - 1.0);
    let mut x = 320.0 - total / 2.0;
    let y = 456.0;
    rrect(x - 10.0, y - 4.0, total + 20.0, 22.0, 11.0, rgba(PANEL, 0.45));
    for ((b, a), (bw, aw)) in items.iter().zip(&widths) {
        rrect(x, y - 1.0, *bw, 16.0, 8.0, rgba(CREAM, 0.88));
        f.text(b, x + bw / 2.0, y, size, PANEL, Align::Center);
        x += bw + 4.0;
        f.text(a, x, y, size, CREAM, Align::Left);
        x += aw + 12.0;
    }
    f.shadowed(s(T::CreditShort, l), 632.0, 434.0, 11.0, rgba(SOFT, 0.75), Align::Right, Weight::Text);
}

/// `h:mm:ss`, or `m:ss` under an hour.
pub fn clock(secs: f64) -> String {
    let t = secs.max(0.0) as u64;
    let (h, m, s) = (t / 3600, t / 60 % 60, t % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

const PLANET_NAMES: [T; 3] = [T::B612, T::Lamp, T::Desert];

/// The statistics of the journey being played, as label and value.
fn statistics(app: &App) -> Vec<(String, String)> {
    let l = app.lang;
    let st = &app.journey.stats;
    let visited = app.journey.visited.iter().filter(|v| **v).count();
    vec![
        (s(T::PlayTime, l).into(), clock(st.time)),
        (s(T::Distance, l).into(), format!("{:.0} {}", st.distance, s(T::Metres, l))),
        (s(T::Jumps, l).into(), st.jumps.to_string()),
        (s(T::SunsetsSeen, l).into(), st.sunsets.to_string()),
        (s(T::SproutsPulled, l).into(), st.sprouts.to_string()),
        (s(T::Flights, l).into(), st.flights.to_string()),
        (s(T::Visited, l).into(), format!("{visited}/3")),
    ]
}

/// A menu page over the scene: a panel with a title, its rows and the
/// buttons to use.
fn menu(app: &App, m: &Menu) {
    let f = &app.fonts;
    let l = app.lang;
    let (page, sel) = m.top();
    let key = |b: &str, t: T| format!("{b} {}", s(t, l));
    let (title, row_h, rows, hint): (T, f32, Vec<(String, String)>, String) = match page {
        Page::Journeys => {
            let rows = app
                .journeys
                .iter()
                .enumerate()
                .map(|(i, j)| {
                    let head = format!("{} {}", s(T::Journey, l), i + 1);
                    match j {
                        Some(j) => {
                            let visited = j.visited.iter().filter(|v| **v).count();
                            (
                                format!("{head} · {}", s(PLANET_NAMES[j.planet], l)),
                                format!(
                                    "{} · {} {visited}/3 · {} {}",
                                    clock(j.stats.time),
                                    s(T::Visited, l),
                                    s(T::SunsetsSeen, l),
                                    j.stats.sunsets
                                ),
                            )
                        }
                        None => (head, s(T::NewJourney, l).into()),
                    }
                })
                .collect();
            let hint = [key("A", T::Choose), key("X", T::Delete), key("B", T::GoBack)].join("   ");
            (T::Journeys, 50.0, rows, hint)
        }
        Page::Pause => {
            let rows = PAUSE
                .iter()
                .map(|p| {
                    let t = match p {
                        PauseItem::Resume => T::Resume,
                        PauseItem::Settings => T::Settings,
                        PauseItem::Statistics => T::Statistics,
                        PauseItem::SaveAndQuit => T::SaveQuit,
                    };
                    (s(t, l).to_string(), String::new())
                })
                .collect();
            (T::Paused, 32.0, rows, [key("A", T::Choose), key("B", T::Resume)].join("   "))
        }
        Page::Settings => {
            let on = |b: bool| s(if b { T::On } else { T::Off }, l).to_string();
            let rows = SETTINGS
                .iter()
                .map(|st| {
                    let (t, v) = match st {
                        Setting::Language => (T::Language, s(T::LangName, l).to_string()),
                        Setting::Effects => {
                            (T::Effects, s([T::FxLow, T::FxMid, T::FxHigh][app.fx_level as usize], l).into())
                        }
                        Setting::Performance => (T::Performance, on(app.show_stats)),
                        Setting::Volume => (T::Volume, format!("{}", app.settings.volume)),
                        Setting::Invert => (T::Invert, on(app.settings.invert)),
                    };
                    (s(t, l).to_string(), format!("<  {v}  >"))
                })
                .collect();
            (T::Settings, 32.0, rows, [format!("←→ {}", s(T::Change, l)), key("B", T::GoBack)].join("   "))
        }
        Page::Statistics => (T::Statistics, 26.0, statistics(app), key("B", T::GoBack)),
    };
    let w = 360.0;
    let h = 52.0 + rows.len() as f32 * row_h + 40.0;
    let (x, y) = (320.0 - w / 2.0, (240.0 - h / 2.0).max(8.0));
    unsafe { ffi::DrawRectangle(0, 0, 640, 480, rgba(PANEL, 0.35)) };
    rrect(x, y, w, h, 14.0, rgba(PANEL, 0.9));
    f.text(s(title, l), 320.0, y + 14.0, 20.0, CREAM, Align::Center);
    let selectable = page != Page::Statistics;
    for (i, (a, b)) in rows.iter().enumerate() {
        let ry = y + 52.0 + i as f32 * row_h;
        let chosen = selectable && i == sel;
        if chosen {
            rrect(x + 12.0, ry - 2.0, w - 24.0, row_h - 4.0, 8.0, rgba(GOLD, 0.22));
        }
        let col = if chosen { rgb(0xFFFFFF) } else { CREAM };
        match page {
            Page::Journeys => {
                f.text(a, x + 26.0, ry + 4.0, 14.0, col, Align::Left);
                f.text(b, x + 26.0, ry + 24.0, 11.0, SOFT, Align::Left);
            }
            Page::Pause => {
                f.text(a, 320.0, ry + 5.0, 14.0, col, Align::Center);
            }
            _ => {
                f.text(a, x + 26.0, ry + 5.0, 13.0, col, Align::Left);
                f.text(b, x + w - 26.0, ry + 5.0, 13.0, if chosen { GOLD } else { SOFT }, Align::Right);
            }
        }
    }
    let (hint, col) = if m.confirm {
        (format!("X {}", s(T::ConfirmDelete, l)), rgb(0xFF8A7A))
    } else if app.store.failed {
        (s(T::SaveFailed, l).to_string(), rgb(0xFF8A7A))
    } else {
        (hint, SOFT)
    };
    f.text(&hint, 320.0, y + h - 28.0, 11.0, col, Align::Center);
}

fn opt(v: Option<f32>, digits: usize) -> String {
    v.map_or_else(|| "--".to_string(), |v| format!("{v:.digits$}"))
}

fn stats(app: &App) {
    let f = &app.fonts;
    let l = app.lang;
    let st = &app.stats;
    let n = st.now;
    let ms = st.frame_ms();
    let fps = if ms > 0.0 { 1000.0 / ms } else { 0.0 };
    let good = |fps: f32| {
        if fps >= 55.0 {
            rgb(0x7CF29A)
        } else if fps >= 30.0 {
            rgb(0xFFD36E)
        } else {
            rgb(0xFF6E6E)
        }
    };
    let (x, y) = (8.0, 8.0);
    rrect(x, y, 222.0, 100.0, 8.0, rgba(PANEL, 0.62));
    let w = f.text(&format!("{fps:.0}"), x + 9.0, y + 4.0, 20.0, good(fps), Align::Left);
    f.text("FPS", x + 12.0 + w, y + 11.0, 11.0, SOFT, Align::Left);
    f.text(&format!("{ms:.1} ms"), x + 82.0, y + 11.0, 11.0, SOFT, Align::Left);
    // Frame times, newest on the right; the line marks 16.7 ms.
    let (gx, gy, gw, gh) = (x + 136.0, y + 8.0, 78.0, 20.0);
    unsafe {
        ffi::DrawRectangleRec(Rectangle { x: gx, y: gy, width: gw, height: gh }, rgba(rgb(0xFFFFFF), 0.06));
        let bw = gw / 90.0;
        let start = 90 - st.frames.len().min(90);
        for (i, m) in st.frames.iter().rev().take(90).rev().enumerate() {
            let hh = (m / 33.3).min(1.0) * gh;
            let c = good(1000.0 / m.max(0.1));
            ffi::DrawRectangleRec(
                Rectangle { x: gx + (start + i) as f32 * bw, y: gy + gh - hh, width: bw.max(1.0), height: hh },
                rgba(c, 0.85),
            );
        }
        ffi::DrawRectangleRec(Rectangle { x: gx, y: gy + gh * 0.5, width: gw, height: 1.0 }, rgba(rgb(0xFFFFFF), 0.3));
    }
    let line = |row: f32, s: &str| {
        f.text(s, x + 9.0, y + 32.0 + row * 15.0, 11.0, CREAM, Align::Left);
    };
    line(0.0, &format!("CPU {}% × {}   {} {}%", opt(n.cpu, 0), n.cores, s(T::Proc, l), opt(n.cpu_self, 1)));
    line(1.0, &format!("{} {} {} / {} MB", s(T::Mem, l), s(T::Free, l), opt(n.mem_free_mb, 0), opt(n.mem_total_mb, 0)));
    let mut sys = format!("{} {} MB", s(T::Proc, l), opt(n.mem_self_mb, 0));
    if let Some(c) = n.temp_c {
        sys += &format!(" · {c:.0}°C");
    }
    if let Some(m) = n.mhz {
        sys += &format!(" · {m:.0} MHz");
    }
    line(2.0, &sys);
    let backend = match unsafe { ffi::rlGetVersion() } {
        v if v == ffi::rlGlVersion::RL_OPENGL_ES_30 as i32 => "GLES 3",
        v if v == ffi::rlGlVersion::RL_OPENGL_ES_20 as i32 => "GLES 2",
        _ => "OpenGL 3.3",
    };
    let backend = if cfg!(feature = "gizlet-os") { format!("{backend} · DRM") } else { backend.to_string() };
    let fx = [T::FxLow, T::FxMid, T::FxHigh][app.fx_level as usize];
    line(3.0, &format!("{backend} · 640×480 · {} {}", s(T::Effects, l), s(fx, l)));
}
