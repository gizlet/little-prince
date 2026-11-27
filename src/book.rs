//! The storybook: the title's cover, and the journeys, the star chart and
//! the planets' chapters as spreads of one illustrated book floating in the
//! night.
//!
//! The paper is made here at start-up (grain, stains, torn edges and
//! watercolour clouds), so the book needs no image files. The planets on its
//! pages are the planets themselves in miniature (`World::mini`): drawn into
//! small textures at twice their size on screen, in colour once visited and
//! as blue ink sketches before that. While the book fills the screen the
//! scene behind it is not drawn at all.

use crate::app::{App, Mode};
use crate::gl::{mix, rgb, rgba, Lighting};
use crate::hud::{clock, rrect, wrap};
use crate::math::{basis, clamp01, smooth, v3, Rng, V3};
use crate::menu::Page;
use crate::save::Slot;
use crate::text::{s, Align, Fonts, Lang, Weight, T};
use raylib_sys::{self as ffi, Camera3D, Color, Rectangle, RenderTexture2D, Texture2D, Vector2};
use std::cell::RefCell;
use std::f32::consts::PI;

const INK: Color = rgb(0x2B3566);
const FADED: Color = rgb(0x7D7F9C);
const GOLD: Color = rgb(0xC8962E);
const LIGHT_GOLD: Color = rgb(0xE9C66C);
const NIGHT: Color = rgb(0x0E1434);
const PAPER: Color = rgb(0xF4E8C8);
const CLOTH: Color = rgb(0x24326E);
const RED: Color = rgb(0xB8473C);
const WHITE: Color = rgb(0xFFFFFF);
const RIBBONS: [Color; 3] = [rgb(0xC0453B), rgb(0x3D6FB5), rgb(0x5B9A55)];

const PLANETS: [T; 3] = [T::B612, T::Lamp, T::Desert];
const CHAPTERS: [T; 3] = [T::Chapter1, T::Chapter2, T::Chapter3];
const STORIES: [T; 3] = [T::Story1, T::Story2, T::Story3];
const TASKS: [T; 3] = [T::B612Hint, T::LampHint, T::DesertHint];

/// The open book on the 640×480 canvas: left, top, width, height.
const SPREAD: (f32, f32, f32, f32) = (18.0, 20.0, 604.0, 440.0);
/// The centres of the left and right pages.
const LEFT: f32 = 169.0;
const RIGHT: f32 = 471.0;
/// The planets on the star chart.
const CHART: [(f32, f32); 3] = [(152.0, 236.0), (320.0, 210.0), (488.0, 236.0)];
/// Seconds the book takes to open, and a page to turn.
const OPEN: f32 = 0.35;
const TURN: f32 = 0.5;

/// Whether a menu page is one of the book's.
pub fn is_book(p: Page) -> bool {
    matches!(p, Page::Journeys | Page::StarMap | Page::Chapter)
}

/// Which spread the book is open at and how far it has opened and turned.
#[derive(Clone, Copy, Debug)]
pub struct State {
    /// The book's page and row, while it is open.
    pub page: Option<(Page, usize)>,
    /// 0 closed, 1 open.
    pub open: f32,
    /// How far the current page turn has gone, 1 when none is under way.
    pub turn: f32,
    /// +1 when turning forward (right to left), -1 back.
    pub dir: f32,
}

impl Default for State {
    fn default() -> State {
        State { page: None, open: 0.0, turn: 1.0, dir: 1.0 }
    }
}

fn depth(p: Page) -> u8 {
    match p {
        Page::Chapter => 2,
        Page::StarMap => 1,
        _ => 0,
    }
}

impl State {
    /// Follows the menu: `top` is its top page, `animate` false cuts
    /// instead of opening and turning (effects level 0).
    pub fn update(&mut self, dt: f32, top: Option<(Page, usize)>, animate: bool) {
        let top = top.filter(|(p, _)| is_book(*p));
        match (self.page, top) {
            (_, None) => {
                self.open = 0.0;
                self.turn = 1.0;
            }
            (None, Some(_)) => {
                self.open = if animate { 0.0 } else { 1.0 };
                self.turn = 1.0;
            }
            (Some(a), Some(b)) if a != b => {
                // A new page, or another chapter; another row on the same
                // page only moves the selection.
                if a.0 != b.0 || b.0 == Page::Chapter {
                    let forward = depth(b.0) > depth(a.0) || (a.0 == b.0 && (b.1 + 3 - a.1) % 3 == 1);
                    self.dir = if forward { 1.0 } else { -1.0 };
                    self.turn = if animate { 0.0 } else { 1.0 };
                }
            }
            _ => {}
        }
        self.page = top;
        if self.page.is_some() {
            self.open = (self.open + dt / OPEN).min(1.0);
            self.turn = (self.turn + dt / TURN).min(1.0);
        }
    }

    /// The state as it will be once it has seen `top`: drawing can come
    /// between a menu change and the next update.
    pub fn view(&self, top: Option<(Page, usize)>, animate: bool) -> State {
        let mut s = *self;
        s.update(0.0, top, animate);
        s
    }

    /// Opens the book at `top` at once, for captures.
    pub fn settle(&mut self, top: Option<(Page, usize)>) {
        self.update(0.0, top, false);
        self.open = 1.0;
        self.turn = 1.0;
    }

    /// Whether the book fills the screen, so the scene behind need not be drawn.
    pub fn covers(&self) -> bool {
        self.page.is_some() && self.open >= 1.0
    }
}

/// A planet to draw small: which, in colour or sketched, and its size on
/// the canvas.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Want {
    planet: usize,
    colour: bool,
    size: f32,
}

/// The planets the book shows this frame, in the order they are drawn.
fn wanted(app: &App) -> Vec<Want> {
    let seen = |p: usize| app.journey.visited[p];
    match app.menu.as_ref().map(|m| m.top()) {
        Some((Page::Journeys, row)) => match &app.journeys[row] {
            Slot::Journey(j) => vec![Want { planet: j.planet, colour: true, size: 170.0 }],
            _ => vec![Want { planet: 0, colour: false, size: 170.0 }],
        },
        Some((Page::StarMap, _)) => (0..3).map(|p| Want { planet: p, colour: seen(p), size: 150.0 }).collect(),
        Some((Page::Chapter, p)) => vec![Want { planet: p, colour: seen(p), size: 230.0 }],
        _ if app.mode == Mode::Title => vec![Want { planet: 0, colour: true, size: 128.0 }],
        _ => Vec::new(),
    }
}

struct Res {
    leaf: Texture2D,
    chart: Texture2D,
    tiles: Vec<RenderTexture2D>,
}

thread_local! {
    static RES: RefCell<Option<Res>> = const { RefCell::new(None) };
}

/// Draws this frame's planets into their textures; call outside any drawing
/// mode, `k` being the canvas zoom.
pub fn prepare(app: &App, k: f32) {
    let wants = wanted(app);
    if wants.is_empty() {
        return;
    }
    RES.with(|r| {
        let mut r = r.borrow_mut();
        let res = r.get_or_insert_with(|| Res { leaf: leaf_texture(), chart: chart_texture(), tiles: Vec::new() });
        for (i, w) in wants.iter().enumerate() {
            let px = (w.size * k * 2.0).round() as i32;
            if res.tiles.get(i).is_some_and(|t| t.texture.width != px) {
                unsafe { ffi::UnloadRenderTexture(res.tiles[i]) };
                res.tiles[i] = target(px);
            } else if res.tiles.len() <= i {
                res.tiles.push(target(px));
            }
            draw_planet(app, res.tiles[i], *w);
        }
    });
}

fn target(px: i32) -> RenderTexture2D {
    unsafe {
        let t = ffi::LoadRenderTexture(px, px);
        ffi::SetTextureFilter(t.texture, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32);
        ffi::SetTextureWrap(t.texture, ffi::TextureWrap::TEXTURE_WRAP_CLAMP as i32);
        t
    }
}

/// The planet turning slowly, its spawn tipped towards the viewer.
fn draw_planet(app: &App, rt: RenderTexture2D, w: Want) {
    let world = &app.worlds[w.planet];
    let mesh = world.mini[if w.colour { 0 } else { 1 }];
    let cam = v3(0.0, 0.0, 4.4);
    let spin = app.clock * 0.2 + w.planet as f32 * 2.1;
    let k = 1.0 / world.kind.radius();
    let turn = |d: V3| d.rotate(V3::Y, spin).rotate(v3(1.0, 0.0, 0.0), 0.55) * k;
    let m = basis(turn(v3(1.0, 0.0, 0.0)), turn(V3::Y), turn(v3(0.0, 0.0, 1.0)), V3::ZERO);
    let far = v3(0.0, 1000.0, 0.0);
    let light = Lighting {
        sun: v3(-0.4, 0.55, 0.75).norm(),
        sun_col: if w.colour { rgb(0xFFF3DC) } else { rgb(0xF4F6FF) },
        shade: if w.colour { rgb(0x9A92C6) } else { rgb(0x5A67A6) },
        sky: rgb(0x6E8FD0),
        rim: rgb(0xFFF1DA),
        fog: WHITE,
        fog_range: (1000.0, 2000.0),
        cam,
        pusher: far,
        wind: V3::ZERO,
        lamp: far,
        lamp_col: rgb(0x000000),
    };
    let camera = Camera3D {
        position: cam.rl(),
        target: V3::ZERO.rl(),
        up: V3::Y.rl(),
        fovy: 36.0,
        projection: ffi::CameraProjection::CAMERA_PERSPECTIVE as i32,
    };
    unsafe {
        ffi::BeginTextureMode(rt);
        ffi::ClearBackground(Color { r: 0, g: 0, b: 0, a: 0 });
        ffi::BeginMode3D(camera);
    }
    app.gl.light(&light, app.clock);
    app.gl.draw(mesh, m, WHITE, 0.0);
    unsafe {
        ffi::rlDrawRenderBatchActive();
        ffi::EndMode3D();
        ffi::EndTextureMode();
    }
}

fn v2(x: f32, y: f32) -> Vector2 {
    Vector2 { x, y }
}

/// A planet texture centred on (`x`, `y`), `size` across.
fn tile(rt: RenderTexture2D, x: f32, y: f32, size: f32, a: f32) {
    let t = rt.texture;
    let v = (clamp01(a) * 255.0) as u8;
    unsafe {
        ffi::BeginBlendMode(ffi::BlendMode::BLEND_ALPHA_PREMULTIPLY as i32);
        ffi::DrawTexturePro(
            t,
            Rectangle { x: 0.0, y: 0.0, width: t.width as f32, height: -(t.height as f32) },
            Rectangle { x: x - size / 2.0, y: y - size / 2.0, width: size, height: size },
            v2(0.0, 0.0),
            0.0,
            Color { r: v, g: v, b: v, a: v },
        );
        ffi::EndBlendMode();
    }
}

/// A triangle in either winding.
fn tri(a: Vector2, b: Vector2, c: Vector2, col: Color) {
    let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    unsafe {
        if cross < 0.0 {
            ffi::DrawTriangle(a, b, c, col);
        } else {
            ffi::DrawTriangle(a, c, b, col);
        }
    }
}

/// A four-pointed twinkle.
fn sparkle(x: f32, y: f32, r: f32, col: Color) {
    let w = r * 0.22;
    for (dx, dy) in [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)] {
        tri(v2(x + dx * r, y + dy * r), v2(x - dy * w, y + dx * w), v2(x + dy * w, y - dx * w), col);
    }
}

/// A five-pointed star.
fn star(x: f32, y: f32, r: f32, col: Color) {
    let p = |k: f32, rr: f32| {
        let a = -PI / 2.0 + k * PI / 5.0;
        v2(x + a.cos() * rr, y + a.sin() * rr)
    };
    for i in 0..5 {
        let k = i as f32 * 2.0;
        tri(v2(x, y), p(k, r), p(k + 1.0, r * 0.45), col);
        tri(v2(x, y), p(k - 1.0, r * 0.45), p(k, r), col);
    }
}

/// A gold rule with a twinkle in the middle.
fn divider(x: f32, y: f32, a: f32) {
    unsafe {
        ffi::DrawLineEx(v2(x - 70.0, y), v2(x - 12.0, y), 1.0, rgba(GOLD, 0.6 * a));
        ffi::DrawLineEx(v2(x + 12.0, y), v2(x + 70.0, y), 1.0, rgba(GOLD, 0.6 * a));
    }
    sparkle(x, y, 6.0, rgba(GOLD, a));
}

/// A text size no larger than `size` at which `text` fits in `width`.
fn fit(f: &Fonts, text: &str, size: f32, width: f32, w: Weight) -> f32 {
    let m = f.measure(text, size, w);
    if m > width {
        size * width / m
    } else {
        size
    }
}

/// Letters spread apart, as on a title page.
fn spaced(t: &str) -> String {
    t.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ")
}

/// The book's name above a page's title, a rule and a line beneath.
fn heading(f: &Fonts, l: Lang, x: f32, y: f32, title: T, sub: T, width: f32, a: f32) {
    f.text(&spaced(s(T::TitleFr, l)), x, y, 11.0, rgba(FADED, 0.8 * a), Align::Center);
    let t = s(title, l);
    let size = fit(f, t, 28.0, width, Weight::Display);
    f.draw(t, x, y + 16.0 + (28.0 - size) * 0.5, size, rgba(INK, a), Align::Center, Weight::Display);
    divider(x, y + 58.0, a);
    f.text(s(sub, l), x, y + 66.0, 11.0, rgba(FADED, a), Align::Center);
}

/// Lines of `text` wrapped to `width`, centred on `x` from `y`; returns the
/// height used.
fn para(f: &Fonts, text: &str, x: f32, y: f32, size: f32, width: f32, col: Color) -> f32 {
    let step = size * 1.55;
    let lines = wrap(f, text, size, width);
    for (i, line) in lines.iter().enumerate() {
        f.text(line, x, y + i as f32 * step, size, col, Align::Center);
    }
    lines.len() as f32 * step
}

/// A button's label on a gold pill.
fn pill(f: &Fonts, text: &str, x: f32, y: f32, a: f32) {
    let w = f.measure(text, 13.0, Weight::Text) + 32.0;
    rrect(x - w / 2.0, y, w, 26.0, 13.0, rgba(GOLD, 0.92 * a));
    f.text(text, x, y + 5.0, 13.0, rgba(rgb(0xFFF8E8), a), Align::Center);
}

/// The help line at the foot of the spread.
fn foot(f: &Fonts, text: &str, col: Color, a: f32) {
    f.text(text, 320.0, 430.0, 11.0, rgba(col, a), Align::Center);
}

/// The book, when the menu's top page is one of its pages.
pub fn draw(app: &App) {
    let Some(m) = &app.menu else { return };
    let (page, row) = m.top();
    if !is_book(page) {
        return;
    }
    let st = app.book.view(Some((page, row)), app.fx_level > 0);
    RES.with(|r| {
        let r = r.borrow();
        let Some(res) = r.as_ref() else { return };
        let bg = smooth(st.open * 1.6);
        unsafe { ffi::DrawRectangle(0, 0, 640, 480, rgba(NIGHT, bg)) };
        night(app.clock, bg);
        let e = 1.0 - (1.0 - st.open).powi(3);
        let sc = 0.55 + 0.45 * e;
        let a = smooth((st.open - 0.25) / 0.75);
        unsafe {
            ffi::rlPushMatrix();
            ffi::rlTranslatef(320.0, 240.0, 0.0);
            ffi::rlScalef(sc, sc, 1.0);
            ffi::rlTranslatef(-320.0, -240.0, 0.0);
        }
        let (x, y, w, h) = SPREAD;
        rrect(x + 6.0, y + 9.0, w, h, 10.0, rgba(rgb(0x000000), 0.35 * smooth(st.open)));
        let paper = rgba(WHITE, smooth(st.open * 2.0));
        if page == Page::StarMap {
            texture(res.chart, Rectangle { x, y, width: w, height: h }, false, paper);
        } else {
            texture(res.leaf, Rectangle { x, y, width: w / 2.0, height: h }, true, paper);
            texture(res.leaf, Rectangle { x: x + w / 2.0, y, width: w / 2.0, height: h }, false, paper);
        }
        let tiles = &res.tiles;
        match page {
            Page::Journeys => journeys(app, row, a, tiles, m.confirm.is_some()),
            Page::StarMap => chart(app, row, a, tiles),
            _ => chapter(app, row, a, tiles),
        }
        if st.turn < 1.0 {
            turning(res.leaf, st.turn, st.dir);
        }
        unsafe { ffi::rlPopMatrix() };
    });
}

fn texture(t: Texture2D, dest: Rectangle, flip: bool, tint: Color) {
    let w = t.width as f32;
    let src = Rectangle { x: 0.0, y: 0.0, width: if flip { -w } else { w }, height: t.height as f32 };
    unsafe { ffi::DrawTexturePro(t, src, dest, v2(0.0, 0.0), 0.0, tint) };
}

/// Stars round the book.
fn night(t: f32, a: f32) {
    let mut rng = Rng::new(5);
    for _ in 0..110 {
        let (x, y) = (rng.range(0.0, 640.0), rng.range(0.0, 480.0));
        let (size, phase) = (rng.range(0.5, 1.5), rng.range(0.0, 6.3));
        let tw = 0.35 + 0.65 * (0.5 + 0.5 * (t * 1.7 + phase).sin());
        unsafe { ffi::DrawCircleV(v2(x, y), size, rgba(rgb(0xDDE4FF), a * tw)) };
    }
    for (i, (x, y)) in [(9.0, 70.0), (631.0, 120.0), (10.0, 400.0), (630.0, 444.0), (600.0, 10.0)].iter().enumerate() {
        let r = 5.0 + 1.5 * (t * 1.3 + i as f32).sin();
        sparkle(*x, *y, r, rgba(LIGHT_GOLD, a));
    }
}

/// A page lifting off one side of the spread and laying itself on the
/// other, `p` of the way over.
fn turning(leaf: Texture2D, p: f32, dir: f32) {
    let (x0, y0, w, h) = SPREAD;
    let half = w / 2.0;
    let g = x0 + half;
    let e = smooth(p);
    let lift = (e * PI).sin();
    let edge = g + dir * half * (e * PI).cos();
    let (l, r) = (edge.min(g), edge.max(g));
    let shadow = rgba(rgb(0x1C1830), 0.28 * lift);
    let clear = rgba(shadow, 0.0);
    let sw = 40.0 * lift;
    unsafe {
        if edge > g {
            ffi::DrawRectangleGradientH(edge as i32, y0 as i32, sw as i32, h as i32, shadow, clear);
        } else {
            ffi::DrawRectangleGradientH((edge - sw) as i32, y0 as i32, sw as i32, h as i32, clear, shadow);
        }
    }
    if r - l < 1.0 {
        return;
    }
    let shade = mix(WHITE, rgb(0xC9B896), lift * 0.5);
    texture(leaf, Rectangle { x: l, y: y0 - lift * 8.0, width: r - l, height: h + lift * 16.0 }, edge < g, shade);
    let fold = ((r - l) * 0.25).min(30.0);
    let dark = rgba(rgb(0x5A4A30), 0.25 * lift);
    unsafe {
        if edge > g {
            ffi::DrawRectangleGradientH(g as i32, y0 as i32, fold as i32, h as i32, dark, rgba(dark, 0.0));
        } else {
            ffi::DrawRectangleGradientH((g - fold) as i32, y0 as i32, fold as i32, h as i32, rgba(dark, 0.0), dark);
        }
    }
}

/// A bookmark ribbon hanging from `y`, `len` long.
fn ribbon(x: f32, y: f32, len: f32, col: Color) {
    unsafe { ffi::DrawRectangleRec(Rectangle { x: x - 5.0, y, width: 10.0, height: len }, col) };
    let b = y + len;
    tri(v2(x - 5.0, b), v2(x + 5.0, b), v2(x - 5.0, b + 7.0), col);
    tri(v2(x + 5.0, b), v2(x + 5.0, b + 7.0), v2(x, b), col);
}

fn journeys(app: &App, sel: usize, a: f32, tiles: &[RenderTexture2D], confirm: bool) {
    let f = &app.fonts;
    let l = app.lang;
    heading(f, l, LEFT, 40.0, T::BookTitle, T::BookSub, 250.0, a);
    for (i, slot) in app.journeys.iter().enumerate() {
        let y = 142.0 + i as f32 * 84.0;
        let chosen = i == sel;
        if chosen {
            rrect(42.0, y - 8.0, 258.0, 72.0, 8.0, rgba(GOLD, 0.16 * a));
        }
        ribbon(56.0, y - 8.0, if chosen { 50.0 } else { 36.0 }, rgba(RIBBONS[i], a));
        let head = format!("{} {}", s(T::Journey, l), i + 1);
        f.text(&head, 74.0, y - 2.0, 16.0, rgba(INK, a), Align::Left);
        let (one, two) = match slot {
            Slot::Journey(j) => (
                s(PLANETS[j.planet], l).to_string(),
                format!("{} · {} {}/3", clock(j.stats.time), s(T::TasksDone, l), j.done.iter().filter(|d| **d).count()),
            ),
            Slot::Empty => (s(T::NewJourney, l).to_string(), String::new()),
            Slot::Unreadable => (s(T::Unreadable, l).to_string(), String::new()),
        };
        let one = wrap(f, &one, 13.0, 214.0);
        for (k, line) in one.iter().take(2).enumerate() {
            f.text(line, 74.0, y + 20.0 + k as f32 * 17.0, 13.0, rgba(INK, 0.9 * a), Align::Left);
        }
        f.text(&two, 74.0, y + 40.0, 11.0, rgba(FADED, a), Align::Left);
    }
    let rx = RIGHT;
    match &app.journeys[sel] {
        Slot::Journey(j) => {
            if let Some(t) = tiles.first() {
                tile(*t, rx, 160.0, 170.0, a);
            }
            f.text(s(PLANETS[j.planet], l), rx, 252.0, 16.0, rgba(INK, a), Align::Center);
            for (i, done) in j.done.iter().enumerate() {
                let col = if *done { rgba(GOLD, a) } else { rgba(FADED, 0.3 * a) };
                star(rx - 36.0 + i as f32 * 36.0, 294.0, 11.0, col);
            }
            let visited = j.visited.iter().filter(|v| **v).count();
            let line = format!("{} {}", s(T::PlayTime, l), clock(j.stats.time));
            f.text(&line, rx, 318.0, 11.0, rgba(FADED, a), Align::Center);
            let line = format!("{} {visited}/3 · {} {}", s(T::Visited, l), s(T::SunsetsSeen, l), j.stats.sunsets);
            f.text(&line, rx, 336.0, 11.0, rgba(FADED, a), Align::Center);
        }
        Slot::Empty => {
            if let Some(t) = tiles.first() {
                tile(*t, rx, 160.0, 170.0, 0.9 * a);
            }
            f.text(s(T::NewJourney, l), rx, 252.0, 16.0, rgba(INK, a), Align::Center);
            para(f, s(T::Story1, l), rx, 282.0, 11.0, 230.0, rgba(FADED, a));
        }
        Slot::Unreadable => {
            para(f, s(T::Unreadable, l), rx, 200.0, 13.0, 230.0, rgba(RED, a));
        }
    }
    let key = |b: &str, t: T| format!("{b} {}", s(t, l));
    if confirm {
        foot(f, &format!("X {}", s(T::ConfirmDelete, l)), RED, a);
    } else if app.store.failed {
        foot(f, s(T::DeleteFailed, l), RED, a);
    } else {
        foot(f, &[key("A", T::Choose), key("X", T::Delete), key("B", T::GoBack)].join("    "), FADED, a);
    }
}

/// What the chart says about planet `p`.
fn status(app: &App, p: usize) -> T {
    if p == app.cur {
        T::Here
    } else if app.journey.done[p] {
        T::TaskDone
    } else if app.route_open(p) {
        T::OpenRoute
    } else {
        T::Locked
    }
}

/// A dotted arc between two planets on the chart.
fn route(from: (f32, f32), to: (f32, f32), col: Color) {
    let ctl = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0 + 64.0);
    let mut t = 0.3;
    while t <= 0.71 {
        let u = 1.0 - t;
        let x = u * u * from.0 + 2.0 * u * t * ctl.0 + t * t * to.0;
        let y = u * u * from.1 + 2.0 * u * t * ctl.1 + t * t * to.1;
        unsafe { ffi::DrawCircleV(v2(x, y), 1.6, col) };
        t += 0.028;
    }
}

fn chart(app: &App, sel: usize, a: f32, tiles: &[RenderTexture2D]) {
    let f = &app.fonts;
    let l = app.lang;
    heading(f, l, 320.0, 34.0, T::StarMap, T::ChartSub, 420.0, a);
    for i in 0..2 {
        let open = app.route_open(i + 1);
        route(CHART[i], CHART[i + 1], rgba(INK, a * if open { 0.55 } else { 0.16 }));
    }
    for (p, &(x, y0)) in CHART.iter().enumerate() {
        let chosen = p == sel;
        let y = if chosen { y0 + (app.clock * 2.2).sin() * 3.0 } else { y0 };
        if p == app.cur {
            unsafe { ffi::DrawRing(v2(x, y), 68.0, 70.0, 0.0, 360.0, 48, rgba(GOLD, 0.8 * a)) };
        }
        if let Some(t) = tiles.get(p) {
            tile(*t, x, y, 150.0, a);
        }
        if chosen {
            sparkle(x + 56.0, y - 56.0, 8.0 + 2.0 * (app.clock * 3.0).sin(), rgba(GOLD, a));
        }
        let name = s(PLANETS[p], l);
        let size = fit(f, name, 13.0, 160.0, Weight::Text);
        f.text(name, x, y0 + 84.0, size, rgba(if chosen { INK } else { FADED }, a), Align::Center);
        let col = if chosen { GOLD } else { FADED };
        f.text(s(status(app, p), l), x, y0 + 102.0, 11.0, rgba(col, a), Align::Center);
    }
    let head = format!("{} · {}", s(CHAPTERS[sel], l), s(PLANETS[sel], l));
    f.text(&head, 320.0, 360.0, 11.0, rgba(FADED, a), Align::Center);
    para(f, s(STORIES[sel], l), 320.0, 378.0, 13.0, 520.0, rgba(INK, a));
    let key = |b: &str, t: T| format!("{b} {}", s(t, l));
    let hint = [format!("←→ {}", s(T::Choose, l)), key("A", T::OpenChapter), key("B", T::CloseBook)];
    foot(f, &hint.join("    "), FADED, a);
}

fn chapter(app: &App, p: usize, a: f32, tiles: &[RenderTexture2D]) {
    let f = &app.fonts;
    let l = app.lang;
    let (lx, ly) = (LEFT, 214.0);
    if p == app.cur {
        unsafe { ffi::DrawRing(v2(lx, ly), 104.0, 106.0, 0.0, 360.0, 64, rgba(GOLD, 0.8 * a)) };
    }
    if let Some(t) = tiles.first() {
        tile(*t, lx, ly, 230.0, a);
    }
    if p == app.cur {
        f.text(s(T::Here, l), lx, 340.0, 11.0, rgba(GOLD, a), Align::Center);
    } else if !app.journey.visited[p] {
        f.text(s(T::Unvisited, l), lx, 340.0, 11.0, rgba(FADED, a), Align::Center);
    }
    let rx = RIGHT;
    f.text(s(CHAPTERS[p], l), rx, 44.0, 13.0, rgba(FADED, a), Align::Center);
    let name = s(PLANETS[p], l);
    let size = fit(f, name, 28.0, 250.0, Weight::Display);
    f.draw(name, rx, 64.0 + (28.0 - size) * 0.5, size, rgba(INK, a), Align::Center, Weight::Display);
    divider(rx, 108.0, a);
    let mut y = 124.0;
    y += para(f, s(STORIES[p], l), rx, y, 13.0, 236.0, rgba(INK, a)) + 14.0;
    f.text(s(T::Task, l), rx, y, 11.0, rgba(FADED, a), Align::Center);
    y += 18.0;
    y += para(f, s(TASKS[p], l), rx, y, 13.0, 236.0, rgba(INK, a)) + 4.0;
    let w = &app.worlds[p];
    if app.journey.done[p] {
        star(rx - f.measure(s(T::TaskDone, l), 13.0, Weight::Text) / 2.0 - 12.0, y + 8.0, 7.0, rgba(GOLD, a));
        f.text(s(T::TaskDone, l), rx, y, 13.0, rgba(GOLD, a), Align::Center);
    } else if app.journey.visited[p] {
        let label = s(w.name().2, l);
        let line =
            if w.total > 0 { format!("{label} {}/{}", w.score, w.total) } else { format!("{label} {}", w.score) };
        f.text(&line, rx, y, 13.0, rgba(INK, 0.8 * a), Align::Center);
    }
    if p == app.cur {
        pill(f, &format!("A  {}", s(T::StayHere, l)), rx, 364.0, a);
    } else if app.route_open(p) {
        pill(f, &format!("A  {}", s(T::FlyThere, l)), rx, 364.0, a);
    } else {
        f.text(s(T::Locked, l), rx, 356.0, 13.0, rgba(RED, 0.85 * a), Align::Center);
        para(f, s(T::LockedWhy, l), rx, 376.0, 11.0, 236.0, rgba(FADED, a));
    }
    let hint = [format!("←→ {}", s(T::TurnPage, l)), format!("B {}", s(T::BackToChart, l))];
    foot(f, &hint.join("    "), FADED, a);
}

/// The closed book on the title, floating and turning a little.
pub fn cover(app: &App, a: f32) {
    let f = &app.fonts;
    let l = app.lang;
    let t = app.clock;
    let ang = (t * 0.4).sin() * 0.38;
    let sx = ang.cos();
    let (cx, cy) = (320.0, 186.0 + (t * 1.1).sin() * 4.0);
    let (w, h) = (232.0, 290.0);
    unsafe {
        ffi::DrawCircleGradient(v2(cx, cy), 210.0, rgba(rgb(0x8FA6FF), 0.16 * a), rgba(rgb(0x8FA6FF), 0.0));
    }
    // The pages' edges show on the side turned away.
    let th = 16.0 * ang.sin();
    let side = cx + w / 2.0 * sx * th.signum();
    let (bx, bw) = if th > 0.0 { (side - 2.0, th + 2.0) } else { (side + th, -th + 2.0) };
    unsafe {
        ffi::DrawRectangleRec(Rectangle { x: bx, y: cy - h / 2.0 + 5.0, width: bw, height: h - 10.0 }, rgba(PAPER, a));
        for k in 1..6 {
            let y = cy - h / 2.0 + 5.0 + k as f32 * (h - 10.0) / 6.0;
            ffi::DrawLineEx(v2(bx, y), v2(bx + bw, y), 0.6, rgba(rgb(0xB9A57E), 0.6 * a));
        }
        ffi::rlPushMatrix();
        ffi::rlTranslatef(cx, cy, 0.0);
        ffi::rlScalef(sx, 1.0, 1.0);
    }
    let (x0, y0) = (-w / 2.0, -h / 2.0);
    rrect(x0, y0, w, h, 8.0, rgba(CLOTH, a));
    rrect(x0, y0, 16.0, h, 6.0, rgba(rgb(0x1A2556), a));
    unsafe {
        ffi::DrawCircleGradient(v2(10.0, -30.0), 150.0, rgba(rgb(0x3A4C96), 0.7 * a), rgba(CLOTH, 0.0));
        let frame = Rectangle { x: x0 + 24.0, y: y0 + 12.0, width: w - 36.0, height: h - 24.0 };
        ffi::DrawRectangleLinesEx(frame, 1.5, rgba(LIGHT_GOLD, 0.85 * a));
        let inner =
            Rectangle { x: frame.x + 5.0, y: frame.y + 5.0, width: frame.width - 10.0, height: frame.height - 10.0 };
        ffi::DrawRectangleLinesEx(inner, 0.8, rgba(LIGHT_GOLD, 0.5 * a));
    }
    let mid = 6.0;
    f.text(&spaced(s(T::TitleFr, l)), mid, y0 + 28.0, 11.0, rgba(LIGHT_GOLD, 0.8 * a), Align::Center);
    let title = s(T::Title, l);
    let size = fit(f, title, 40.0, 170.0, Weight::Display);
    let title = if l == Lang::Zh { spaced(title) } else { title.to_string() };
    f.draw(&title, mid, y0 + 46.0, size, rgba(LIGHT_GOLD, a), Align::Center, Weight::Display);
    let other = s(T::Title, if l == Lang::Zh { Lang::En } else { Lang::Zh });
    f.text(other, mid, y0 + 98.0, 13.0, rgba(LIGHT_GOLD, 0.75 * a), Align::Center);
    RES.with(|r| {
        if let Some(t) = r.borrow().as_ref().and_then(|r| r.tiles.first()) {
            unsafe { ffi::DrawRing(v2(mid, 52.0), 62.0, 63.5, 0.0, 360.0, 48, rgba(LIGHT_GOLD, 0.7 * a)) };
            tile(*t, mid, 52.0, 128.0, a);
        }
    });
    for (i, (x, y, r)) in
        [(-70.0, -10.0, 6.0), (78.0, 20.0, 5.0), (-62.0, 110.0, 4.0), (70.0, 118.0, 7.0)].iter().enumerate()
    {
        let tw = 0.6 + 0.4 * (t * 2.0 + i as f32 * 1.7).sin();
        sparkle(*x, *y, *r, rgba(LIGHT_GOLD, a * tw));
    }
    unsafe { ffi::rlPopMatrix() };
}

/// Value noise on a wrapping lattice, sampled at (`x`, `y`) in 0..1.
struct Noise {
    cells: usize,
    v: Vec<f32>,
}

impl Noise {
    fn new(rng: &mut Rng, cells: usize) -> Noise {
        Noise { cells, v: (0..cells * cells).map(|_| rng.f()).collect() }
    }

    fn at(&self, x: f32, y: f32) -> f32 {
        let n = self.cells;
        let (fx, fy) = (x.rem_euclid(1.0) * n as f32, y.rem_euclid(1.0) * n as f32);
        let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = (smooth(fx.fract()), smooth(fy.fract()));
        let at = |i: usize, j: usize| self.v[(j % n) * n + (i % n)];
        let a = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * tx;
        let b = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * tx;
        a + (b - a) * ty
    }
}

struct Fbm(Vec<(Noise, f32)>);

impl Fbm {
    fn new(seed: u64) -> Fbm {
        let mut rng = Rng::new(seed);
        Fbm([(4, 0.5), (9, 0.25), (20, 0.15), (48, 0.1)].iter().map(|&(c, w)| (Noise::new(&mut rng, c), w)).collect())
    }

    fn at(&self, x: f32, y: f32) -> f32 {
        self.0.iter().map(|(n, w)| n.at(x, y) * w).sum()
    }
}

/// Aged paper `w`×`h` pixels. `torn` gives the distance in pixels from
/// (`x`, `y`) to the edges that are torn; `extra` paints over the paper.
fn paper(
    w: usize,
    h: usize,
    seed: u64,
    torn: impl Fn(f32, f32) -> f32,
    extra: impl Fn(f32, f32, Color) -> Color,
) -> Texture2D {
    let grain = Fbm::new(seed);
    let stain = Fbm::new(seed + 1);
    let rough = Fbm::new(seed + 2);
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            let mut c = crate::gl::scale(PAPER, 0.95 + 0.1 * grain.at(u * 2.0, v * 2.0));
            c = mix(c, rgb(0xE6CF9C), smooth((stain.at(u * 0.8 + 0.3, v * 0.8) - 0.5) / 0.2) * 0.45);
            c = extra(u, v, c);
            let d = torn(x as f32, y as f32) - 6.0 + (rough.at(u * 3.0, v * 3.0) - 0.5) * 18.0;
            c = mix(rgb(0xB8925A), c, smooth((d - 1.0) / 10.0) * 0.6 + 0.4);
            let alpha = smooth((d + 1.0) / 2.0);
            let i = (y * w + x) * 4;
            px[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, (alpha * 255.0) as u8]);
        }
    }
    let img = ffi::Image {
        data: px.as_mut_ptr() as _,
        width: w as i32,
        height: h as i32,
        mipmaps: 1,
        format: ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
    };
    unsafe {
        let t = ffi::LoadTextureFromImage(img);
        ffi::SetTextureFilter(t, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32);
        ffi::SetTextureWrap(t, ffi::TextureWrap::TEXTURE_WRAP_CLAMP as i32);
        t
    }
}

/// A right-hand page: the gutter on the left, torn at the other edges.
fn leaf_texture() -> Texture2D {
    let (w, h) = (256usize, 372usize);
    paper(
        w,
        h,
        31,
        |x, y| (w as f32 - 1.0 - x).min(y).min(h as f32 - 1.0 - y),
        |u, _, c| mix(c, rgb(0xB59E72), (1.0 - u / 0.12).max(0.0).powi(2) * 0.55),
    )
}

/// The star chart: one sheet torn all round, with watercolour clouds.
fn chart_texture() -> Texture2D {
    let (w, h) = (512usize, 372usize);
    let wash = Fbm::new(77);
    let clouds = [(0.1, 0.24, 0.1), (0.86, 0.2, 0.12), (0.42, 0.93, 0.12), (0.93, 0.76, 0.1), (0.06, 0.84, 0.09)];
    paper(
        w,
        h,
        43,
        |x, y| x.min(w as f32 - 1.0 - x).min(y).min(h as f32 - 1.0 - y),
        |u, v, c| {
            let aspect = w as f32 / h as f32;
            let mut k: f32 = 0.0;
            for (cx, cy, r) in clouds {
                let d = (((u - cx) * aspect).powi(2) + (v - cy).powi(2)).sqrt() / (r * aspect * 0.7);
                k = k.max(1.0 - d + (wash.at(u * 2.0, v * 2.0) - 0.45) * 1.2);
            }
            let body = smooth(k / 0.5) * 0.45;
            let rim = smooth(1.0 - (k - 0.05).abs() / 0.06) * 0.3;
            mix(mix(c, rgb(0xD3E0F6), body), rgb(0xA9C0E8), rim * smooth(k / 0.02))
        },
    )
}
