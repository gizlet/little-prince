//! Interface strings in Simplified Chinese and English, and font atlases
//! holding exactly their characters.
//!
//! Latin text uses Inter SemiBold and titles Space Grotesk Bold (both SIL Open
//! Font License); Chinese, Japanese and Korean come from the full Droid Sans
//! Fallback (Apache 2.0). raylib reads one font file per atlas, so each atlas
//! is assembled here, as in gizlet/os's launcher: glyphs below U+2E80 come from
//! the Latin face, the rest from Droid, shifted onto the Latin face's baseline.

use raylib_sys::{self as ffi, Color, Font, GlyphInfo, Rectangle, Vector2};
use std::collections::BTreeSet;
use std::ffi::{CStr, CString};

static INTER: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
static GROTESK: &[u8] = include_bytes!("../assets/fonts/SpaceGrotesk-Bold.ttf");
static DROID: &[u8] = include_bytes!("../assets/fonts/DroidSansFallbackFull.ttf");

const CJK_START: i32 = 0x2E80;
const PADDING: i32 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Zh,
    En,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum T {
    Title,
    Subtitle,
    PressStart,
    Credit,
    CreditUrl,
    Book,
    TestNote,
    CreditShort,
    B612,
    B612Hint,
    Lamp,
    LampHint,
    Desert,
    DesertHint,
    Sprouts,
    Sunsets,
    Tamed,
    Jump,
    Travel,
    Camera,
    RStick,
    Effects,
    Autoplay,
    Warping,
    HoldToQuit,
    FxHigh,
    FxMid,
    FxLow,
    Proc,
    Mem,
    Free,
    Menu,
    Statistics,
    Settings,
    Journeys,
    Journey,
    NewJourney,
    Visited,
    Choose,
    GoBack,
    Delete,
    ConfirmDelete,
    Paused,
    Resume,
    SaveQuit,
    SaveFailed,
    Language,
    LangName,
    Performance,
    Volume,
    Invert,
    On,
    Off,
    Change,
    PlayTime,
    Distance,
    Metres,
    Jumps,
    SunsetsSeen,
    SproutsPulled,
    Flights,
    DeleteFailed,
    Unreadable,
    Rose,
    Keeper,
    Fox,
    RoseIntro1,
    RoseIntro2,
    RoseDone1,
    RoseDone2,
    RoseIdle,
    KeeperIntro1,
    KeeperIntro2,
    KeeperDone1,
    KeeperDone2,
    KeeperIdle,
    FoxIntro1,
    FoxIntro2,
    FoxDone1,
    FoxDone2,
    FoxIdle,
    StarMap,
    Here,
    TaskDone,
    OpenRoute,
    Locked,
    Talk,
    FlyThere,
    TasksDone,
    Next,
    TitleFr,
    BookTitle,
    BookSub,
    ChartSub,
    Chapter1,
    Chapter2,
    Chapter3,
    Story1,
    Story2,
    Story3,
    OpenChapter,
    CloseBook,
    BackToChart,
    LockedWhy,
    Task,
    StayHere,
    Unvisited,
    TurnPage,
}

const ALL: [T; 108] = [
    T::Title,
    T::Subtitle,
    T::PressStart,
    T::Credit,
    T::CreditUrl,
    T::Book,
    T::TestNote,
    T::CreditShort,
    T::B612,
    T::B612Hint,
    T::Lamp,
    T::LampHint,
    T::Desert,
    T::DesertHint,
    T::Sprouts,
    T::Sunsets,
    T::Tamed,
    T::Jump,
    T::Travel,
    T::Camera,
    T::RStick,
    T::Effects,
    T::Autoplay,
    T::Warping,
    T::HoldToQuit,
    T::FxHigh,
    T::FxMid,
    T::FxLow,
    T::Proc,
    T::Mem,
    T::Free,
    T::Menu,
    T::Statistics,
    T::Settings,
    T::Journeys,
    T::Journey,
    T::NewJourney,
    T::Visited,
    T::Choose,
    T::GoBack,
    T::Delete,
    T::ConfirmDelete,
    T::Paused,
    T::Resume,
    T::SaveQuit,
    T::SaveFailed,
    T::Language,
    T::LangName,
    T::Performance,
    T::Volume,
    T::Invert,
    T::On,
    T::Off,
    T::Change,
    T::PlayTime,
    T::Distance,
    T::Metres,
    T::Jumps,
    T::SunsetsSeen,
    T::SproutsPulled,
    T::Flights,
    T::DeleteFailed,
    T::Unreadable,
    T::Rose,
    T::Keeper,
    T::Fox,
    T::RoseIntro1,
    T::RoseIntro2,
    T::RoseDone1,
    T::RoseDone2,
    T::RoseIdle,
    T::KeeperIntro1,
    T::KeeperIntro2,
    T::KeeperDone1,
    T::KeeperDone2,
    T::KeeperIdle,
    T::FoxIntro1,
    T::FoxIntro2,
    T::FoxDone1,
    T::FoxDone2,
    T::FoxIdle,
    T::StarMap,
    T::Here,
    T::TaskDone,
    T::OpenRoute,
    T::Locked,
    T::Talk,
    T::FlyThere,
    T::TasksDone,
    T::Next,
    T::TitleFr,
    T::BookTitle,
    T::BookSub,
    T::ChartSub,
    T::Chapter1,
    T::Chapter2,
    T::Chapter3,
    T::Story1,
    T::Story2,
    T::Story3,
    T::OpenChapter,
    T::CloseBook,
    T::BackToChart,
    T::LockedWhy,
    T::Task,
    T::StayHere,
    T::Unvisited,
    T::TurnPage,
];

pub fn s(t: T, l: Lang) -> &'static str {
    let (zh, en) = match t {
        T::Title => ("小王子", "The Little Prince"),
        T::Subtitle => ("星球漫游 · Gizlet OS 上的 3D 游戏", "A planet walk · a 3D game for Gizlet OS"),
        T::PressStart => ("按 A 翻开书", "Press A to open the book"),
        T::Credit => ("致谢：角色模型来自 Kay Lousberg 的 KayKit Adventurers（CC0）", "Thanks to Kay Lousberg for the hero, from KayKit Adventurers (CC0)"),
        T::CreditUrl => ("www.kaylousberg.com", "www.kaylousberg.com"),
        T::Book => ("原著：安托万·德·圣-埃克苏佩里《小王子》（1943）", "After Antoine de Saint-Exupéry's The Little Prince (1943)"),
        T::TestNote => ("Gizlet OS 上用 Rust + raylib 从零编写的 3D 游戏，其余模型与特效均由代码生成", "A 3D game for Gizlet OS written from scratch in Rust + raylib; every other model and effect is made in code"),
        T::CreditShort => ("角色：KayKit · Kay Lousberg", "Hero: KayKit by Kay Lousberg"),
        T::B612 => ("B612 · 小王子的星球", "B612 · The Prince's Planet"),
        T::B612Hint => ("拔掉猴面包树的幼苗，再去看看玫瑰", "Pull up the baobab sprouts, then visit the rose"),
        T::Lamp => ("点灯人的星球", "The Lamplighter's Planet"),
        T::LampHint => ("陪点灯人看三次日落，再和他说说话", "Watch three sunsets with the lamplighter, then talk to him"),
        T::Desert => ("地球 · 沙漠", "Earth · The Desert"),
        T::DesertHint => ("停下来，安静地等狐狸走近", "Stand still and let the fox come closer"),
        T::Sprouts => ("幼苗", "Sprouts"),
        T::Sunsets => ("日落", "Sunsets"),
        T::Tamed => ("驯服", "Tamed"),
        T::Jump => ("跳跃", "Jump"),
        T::Travel => ("星图", "Star map"),
        T::Camera => ("视角", "Camera"),
        T::RStick => ("右摇杆", "R-stick"),
        T::Effects => ("特效", "Effects"),
        T::Autoplay => ("自动游玩中 · 按任意键接管", "Autoplay · press any button to play"),
        T::Warping => ("候鸟带他飞向下一颗星球", "Migrating birds carry him to the next planet"),
        T::HoldToQuit => ("继续按住以退出", "Keep holding to quit"),
        T::FxHigh => ("高", "High"),
        T::FxMid => ("中", "Mid"),
        T::FxLow => ("低", "Low"),
        T::Proc => ("本程序", "App"),
        T::Mem => ("内存", "RAM"),
        T::Free => ("可用", "free"),
        T::Menu => ("菜单", "Menu"),
        T::Statistics => ("统计", "Statistics"),
        T::Settings => ("设置", "Settings"),
        T::Journeys => ("选择旅程", "Choose a journey"),
        T::Journey => ("旅程", "Journey"),
        T::NewJourney => ("开始新的旅程", "Start a new journey"),
        T::Visited => ("去过的星球", "Planets visited"),
        T::Choose => ("选择", "Choose"),
        T::GoBack => ("返回", "Back"),
        T::Delete => ("删除", "Delete"),
        T::ConfirmDelete => ("再按 X 删除这段旅程", "Press X again to delete this journey"),
        T::Paused => ("暂停", "Paused"),
        T::Resume => ("继续", "Resume"),
        T::SaveQuit => ("存档并回到标题", "Save and return to the title"),
        T::SaveFailed => ("存档失败，进度没有写入", "Could not save: progress was not written"),
        T::Language => ("语言", "Language"),
        T::LangName => ("中文", "English"),
        T::Performance => ("性能面板", "Performance panel"),
        T::Volume => ("音量", "Volume"),
        T::Invert => ("镜头上下反转", "Invert camera"),
        T::On => ("开", "On"),
        T::Off => ("关", "Off"),
        T::Change => ("调整", "Change"),
        T::PlayTime => ("游戏时间", "Time played"),
        T::Distance => ("走过的路", "Distance walked"),
        T::Metres => ("米", "m"),
        T::Jumps => ("跳跃", "Jumps"),
        T::SunsetsSeen => ("看过的日落", "Sunsets watched"),
        T::SproutsPulled => ("拔掉的幼苗", "Sprouts pulled"),
        T::Flights => ("飞行", "Flights"),
        T::DeleteFailed => ("删除失败，这段旅程还在", "Could not delete: the journey is still there"),
        T::Unreadable => ("存档无法读取：按两次 X 移开它", "Cannot read this save: press X twice to move it aside"),
        T::Rose => ("玫瑰", "The rose"),
        T::Keeper => ("点灯人", "The lamplighter"),
        T::Fox => ("狐狸", "The fox"),
        T::RoseIntro1 => ("你回来了。猴面包树苗又冒出来了，再长下去会把星球撑裂的。", "You're back. The baobab sprouts are coming up again, and they could split the planet apart."),
        T::RoseIntro2 => ("趁它们还小，把七棵都拔掉吧。我就在这里等你。", "Pull up all seven while they're small. I'll be right here."),
        T::RoseDone1 => ("星球干净了。谢谢你，我一直知道你会照顾我。", "The planet is clear. Thank you. I always knew you would look after me."),
        T::RoseDone2 => ("去看看别的星星吧。候鸟会带你去点灯人那里。", "Go and see the other stars. The birds will take you to the lamplighter."),
        T::RoseIdle => ("别担心我，我有四根刺呢。", "Don't worry about me. I have my four thorns."),
        T::KeeperIntro1 => ("早上好……晚上好。这里转得太快，灯刚点上又该熄了。", "Good morning... good evening. This planet turns so fast, the lamp is barely lit before it must go out."),
        T::KeeperIntro2 => ("陪我看三次日落吧。我一个人看，总是数不清。", "Watch three sunsets with me. On my own I always lose count."),
        T::KeeperDone1 => ("三次日落，你一点也不嫌烦。你是第一个。", "Three sunsets, and you never once grew tired. You're the first."),
        T::KeeperDone2 => ("听说地球很大，那里的沙漠里住着一只狐狸。", "They say Earth is very big, and a fox lives in its desert."),
        T::KeeperIdle => ("该点灯了。再见……不，晚上好。", "Time to light the lamp. Goodbye... no, good evening."),
        T::FoxIntro1 => ("我还不能和你玩，我还没有被驯养。", "I can't play with you yet. I haven't been tamed."),
        T::FoxIntro2 => ("每次都坐得近一点，什么也别说。慢慢来。", "Sit a little closer each time, and say nothing. Take your time."),
        T::FoxDone1 => ("现在，在所有人里，我只认得你的脚步声了。", "Now, of everyone, yours are the only footsteps I know."),
        T::FoxDone2 => ("我跟你走。以后看见麦田，我就会想起你。", "I'll come with you. From now on, the wheat will remind me of you."),
        T::FoxIdle => ("走吧，我就在你身后。", "Go on. I'm right behind you."),
        T::StarMap => ("星图", "Star map"),
        T::Here => ("你在这里", "You are here"),
        T::TaskDone => ("任务完成", "Task done"),
        T::OpenRoute => ("可以前往", "Open"),
        T::Locked => ("还没有航线", "No route yet"),
        T::Talk => ("交谈", "Talk"),
        T::FlyThere => ("飞往", "Fly there"),
        T::TasksDone => ("完成的任务", "Tasks done"),
        T::Next => ("下一句", "Next"),
        T::TitleFr => ("LE PETIT PRINCE", "LE PETIT PRINCE"),
        T::BookTitle => ("小王子的旅程", "The Little Prince's Journeys"),
        T::BookSub => ("每一段旅程都夹着一枚书签", "Each journey keeps its own bookmark"),
        T::ChartSub => ("候鸟的航线连起三颗星星", "The birds' routes join three stars"),
        T::Chapter1 => ("第一章", "Chapter One"),
        T::Chapter2 => ("第二章", "Chapter Two"),
        T::Chapter3 => ("第三章", "Chapter Three"),
        T::Story1 => ("一颗小小的星球，三座火山，一朵骄傲的玫瑰，还有总也拔不完的树苗。", "A small planet, three volcanoes, a proud rose, and sprouts that never stop coming."),
        T::Story2 => ("这里的一天只有十四秒。点灯人点亮了灯，又熄灭它，从来没有歇过。", "Here a day lasts fourteen seconds. The lamplighter lights his lamp and puts it out again, and never rests."),
        T::Story3 => ("沙漠里停着一架坏了的飞机，麦田边有一只狐狸，在等一个愿意慢慢来的人。", "A broken aeroplane rests in the desert, and by the wheat a fox waits for someone who will take their time."),
        T::OpenChapter => ("翻开这一章", "Open the chapter"),
        T::CloseBook => ("合上书", "Close the book"),
        T::BackToChart => ("回到星图", "Back to the chart"),
        T::LockedWhy => ("完成上一颗星球的任务，航线就会打开", "The route opens once the planet before is done"),
        T::Task => ("任务", "Task"),
        T::StayHere => ("留在这里", "Stay here"),
        T::Unvisited => ("还没去过", "Not visited yet"),
        T::TurnPage => ("翻页", "Turn the page"),
    };
    match l {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

/// Characters the HUD draws outside the tables above.
const EXTRA: &str = "·×°%/:.-+→←…（）《》：，、“”éÉ ";

pub fn codepoints() -> BTreeSet<i32> {
    // '|' is left out: Inter's is taller than the atlas line.
    let mut set: BTreeSet<i32> = (32..127).filter(|&c| c != '|' as i32).collect();
    for t in ALL {
        for l in [Lang::Zh, Lang::En] {
            set.extend(s(t, l).chars().map(|c| c as i32));
        }
    }
    set.extend(EXTRA.chars().map(|c| c as i32));
    set
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weight {
    Text,
    Display,
}

/// The baseline as a fraction of the pixel height raylib scales a face to,
/// from the `hhea` table: ascender / (ascender - descender).
fn baseline(font: &[u8]) -> f32 {
    let u16_at = |o: usize| font.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let u32_at = |o: usize| font.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let tables = u16_at(4).unwrap_or(0) as usize;
    for i in 0..tables {
        let entry = 12 + i * 16;
        if font.get(entry..entry + 4) == Some(b"hhea") {
            let Some(offset) = u32_at(entry + 8) else { break };
            let ascender = u16_at(offset as usize + 4).unwrap_or(0) as i16 as f32;
            let descender = u16_at(offset as usize + 6).unwrap_or(0) as i16 as f32;
            if ascender - descender > 0.0 {
                return ascender / (ascender - descender);
            }
        }
    }
    0.8
}

fn glyphs(font: &[u8], px: i32, codepoints: &[i32]) -> Vec<GlyphInfo> {
    if codepoints.is_empty() {
        return Vec::new();
    }
    let mut count = 0;
    unsafe {
        let ptr = ffi::LoadFontData(
            font.as_ptr(),
            font.len() as i32,
            px,
            codepoints.as_ptr(),
            codepoints.len() as i32,
            ffi::FontType::FONT_DEFAULT as i32,
            &mut count,
        );
        if ptr.is_null() {
            return Vec::new();
        }
        let out = std::slice::from_raw_parts(ptr, count.max(0) as usize).to_vec();
        // The glyph images now belong to `out`; only the array is freed.
        ffi::MemFree(ptr.cast());
        out
    }
}

fn build(latin: &[u8], px: i32, codepoints: &BTreeSet<i32>) -> Font {
    let (lat, cjk): (Vec<i32>, Vec<i32>) = codepoints.iter().partition(|&&c| c < CJK_START);
    let mut all = glyphs(latin, px, &lat);
    let shift = ((baseline(latin) - baseline(DROID)) * px as f32).round() as i32;
    for mut g in glyphs(DROID, px, &cjk) {
        g.offsetY += shift;
        all.push(g);
    }
    unsafe {
        let size = std::mem::size_of::<GlyphInfo>() * all.len().max(1);
        let array = ffi::MemAlloc(size as u32) as *mut GlyphInfo;
        std::ptr::copy_nonoverlapping(all.as_ptr(), array, all.len());
        let mut recs: *mut Rectangle = std::ptr::null_mut();
        let atlas = ffi::GenImageFontAtlas(array, &mut recs, all.len() as i32, px, PADDING, 0);
        let texture = ffi::LoadTextureFromImage(atlas);
        ffi::UnloadImage(atlas);
        ffi::SetTextureFilter(texture, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32);
        Font { baseSize: px, glyphCount: all.len() as i32, glyphPadding: PADDING, texture, recs, glyphs: array }
    }
}

/// Atlases at a few sizes, rasterized at the UI zoom so text stays sharp on
/// a large desktop window. Sizes are in 640×480 canvas pixels.
pub struct Fonts {
    faces: Vec<(Weight, f32, Font)>,
    zoom: f32,
}

impl Fonts {
    pub fn new(zoom: f32) -> Fonts {
        let set = codepoints();
        let mut faces = Vec::new();
        for (w, sizes, face) in
            [(Weight::Text, &[11.0, 13.0, 16.0, 20.0][..], INTER), (Weight::Display, &[28.0, 56.0][..], GROTESK)]
        {
            for &px in sizes {
                faces.push((w, px, build(face, (px * zoom).round() as i32, &set)));
            }
        }
        Fonts { faces, zoom }
    }

    fn pick(&self, w: Weight, size: f32) -> Font {
        let mut best = None;
        for (fw, px, f) in &self.faces {
            if *fw == w {
                best = Some(*f);
                if *px >= size - 0.5 {
                    break;
                }
            }
        }
        best.expect("fonts")
    }

    pub fn measure(&self, s: &str, size: f32, w: Weight) -> f32 {
        let text = CString::new(s).unwrap_or_default();
        Self::width(self.pick(w, size), &text, size)
    }

    fn width(font: Font, text: &CStr, size: f32) -> f32 {
        unsafe { ffi::MeasureTextEx(font, text.as_ptr(), size, 0.0).x }
    }

    fn draw_text(&self, font: Font, text: &CStr, x: f32, y: f32, size: f32, color: Color) {
        // Snap to device pixels so small text does not blur.
        let snap = |v: f32| (v * self.zoom).round() / self.zoom;
        unsafe { ffi::DrawTextEx(font, text.as_ptr(), Vector2 { x: snap(x), y: snap(y) }, size, 0.0, color) };
    }

    fn align_x(x: f32, width: f32, align: Align) -> f32 {
        match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        }
    }

    pub fn draw(&self, s: &str, x: f32, y: f32, size: f32, color: Color, align: Align, w: Weight) -> f32 {
        let text = CString::new(s).unwrap_or_default();
        let font = self.pick(w, size);
        let width = Self::width(font, &text, size);
        self.draw_text(font, &text, Self::align_x(x, width, align), y, size, color);
        width
    }

    pub fn text(&self, s: &str, x: f32, y: f32, size: f32, color: Color, align: Align) -> f32 {
        self.draw(s, x, y, size, color, align, Weight::Text)
    }

    /// Text with a soft dark shadow, for words over the moving scene.
    pub fn shadowed(&self, s: &str, x: f32, y: f32, size: f32, color: Color, align: Align, w: Weight) -> f32 {
        let sh = Color { r: 6, g: 8, b: 22, a: (color.a as f32 * 0.55) as u8 };
        let text = CString::new(s).unwrap_or_default();
        let font = self.pick(w, size);
        let width = Self::width(font, &text, size);
        self.draw_text(font, &text, Self::align_x(x + 1.0, width, align), y + 1.5, size, sh);
        self.draw_text(font, &text, Self::align_x(x, width, align), y, size, color);
        width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_has_both_languages() {
        for t in ALL {
            assert!(!s(t, Lang::Zh).is_empty() && !s(t, Lang::En).is_empty(), "{t:?}");
        }
    }

    #[test]
    fn the_credit_names_the_source() {
        assert_eq!(s(T::CreditUrl, Lang::Zh), "www.kaylousberg.com");
    }

    #[test]
    fn reads_baselines() {
        for font in [INTER, GROTESK, DROID] {
            let b = baseline(font);
            assert!(b > 0.6 && b < 0.95, "{b}");
        }
    }
}
