//! The menus over the game: choosing a journey on the title, the pause menu,
//! settings and statistics. This is only the state and what each button
//! does; `App` carries out the actions and `hud` draws the menus.

/// A menu page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    /// The three journeys, on the title.
    Journeys,
    Pause,
    Settings,
    Statistics,
}

/// The pause menu's rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PauseItem {
    Resume,
    Settings,
    Statistics,
    SaveAndQuit,
}

pub const PAUSE: [PauseItem; 4] =
    [PauseItem::Resume, PauseItem::Settings, PauseItem::Statistics, PauseItem::SaveAndQuit];

/// The settings' rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Setting {
    Language,
    Effects,
    Performance,
    Volume,
    Invert,
}

pub const SETTINGS: [Setting; 5] =
    [Setting::Language, Setting::Effects, Setting::Performance, Setting::Volume, Setting::Invert];

/// What a button press asks the game to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    None,
    /// The menu closed: back to the title's prompt or to play.
    Close,
    /// Start or continue journey `n`.
    Load(usize),
    /// Move journey `n` aside.
    Clear(usize),
    /// Save and return to the title.
    SaveAndQuit,
    /// Change a setting one step, `-1` or `+1`.
    Change(Setting, i32),
}

/// The buttons a menu reacts to, pressed this frame.
#[derive(Clone, Copy, Default, Debug)]
pub struct Keys {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub a: bool,
    pub b: bool,
    pub x: bool,
    pub start: bool,
}

/// The open pages, the last on top, each with its selected row.
#[derive(Clone, Debug)]
pub struct Menu {
    stack: Vec<(Page, usize)>,
    /// On the journeys page: X was pressed once on the selected journey and
    /// a second press deletes it.
    pub confirm: bool,
}

impl Menu {
    pub fn open(page: Page, row: usize) -> Menu {
        Menu { stack: vec![(page, row.min(Self::rows(page).saturating_sub(1)))], confirm: false }
    }

    /// The page on top and its selected row.
    pub fn top(&self) -> (Page, usize) {
        *self.stack.last().unwrap_or(&(Page::Pause, 0))
    }

    fn rows(page: Page) -> usize {
        match page {
            Page::Journeys => crate::save::SLOTS,
            Page::Pause => PAUSE.len(),
            Page::Settings => SETTINGS.len(),
            Page::Statistics => 0,
        }
    }

    /// Goes back a page; `Close` when none is left.
    fn back(&mut self) -> Action {
        self.stack.pop();
        if self.stack.is_empty() {
            Action::Close
        } else {
            Action::None
        }
    }

    pub fn step(&mut self, k: Keys) -> Action {
        let Some((page, row)) = self.stack.last_mut() else { return Action::Close };
        let (page, rows) = (*page, Self::rows(*page));
        if rows > 0 {
            if k.up {
                *row = (*row + rows - 1) % rows;
            }
            if k.down {
                *row = (*row + 1) % rows;
            }
        }
        let row = *row;
        let confirm = std::mem::take(&mut self.confirm);
        if k.start && page != Page::Journeys {
            self.stack.clear();
            return Action::Close;
        }
        if k.b {
            return self.back();
        }
        match page {
            Page::Journeys => {
                if k.a {
                    Action::Load(row)
                } else if k.x && confirm {
                    Action::Clear(row)
                } else {
                    self.confirm = k.x || (confirm && !(k.up || k.down || k.a || k.b || k.start));
                    Action::None
                }
            }
            Page::Pause if k.a => match PAUSE[row] {
                PauseItem::Resume => self.back(),
                PauseItem::Settings => {
                    self.stack.push((Page::Settings, 0));
                    Action::None
                }
                PauseItem::Statistics => {
                    self.stack.push((Page::Statistics, 0));
                    Action::None
                }
                PauseItem::SaveAndQuit => Action::SaveAndQuit,
            },
            Page::Settings if k.left => Action::Change(SETTINGS[row], -1),
            Page::Settings if k.right || k.a => Action::Change(SETTINGS[row], 1),
            Page::Statistics if k.a => self.back(),
            _ => Action::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(f: impl Fn(&mut Keys)) -> Keys {
        let mut k = Keys::default();
        f(&mut k);
        k
    }

    #[test]
    fn the_pause_menu_opens_settings_and_comes_back() {
        let mut m = Menu::open(Page::Pause, 0);
        assert_eq!(m.step(press(|k| k.down = true)), Action::None);
        assert_eq!(m.top(), (Page::Pause, 1));
        assert_eq!(m.step(press(|k| k.a = true)), Action::None);
        assert_eq!(m.top(), (Page::Settings, 0));
        assert_eq!(m.step(press(|k| k.right = true)), Action::Change(Setting::Language, 1));
        m.step(press(|k| k.down = true));
        assert_eq!(m.step(press(|k| k.left = true)), Action::Change(Setting::Effects, -1));
        assert_eq!(m.step(press(|k| k.b = true)), Action::None);
        assert_eq!(m.top(), (Page::Pause, 1));
        assert_eq!(m.step(press(|k| k.b = true)), Action::Close);
    }

    #[test]
    fn start_closes_the_pause_menu_from_any_page() {
        let mut m = Menu::open(Page::Pause, 2);
        m.step(press(|k| k.a = true));
        assert_eq!(m.top(), (Page::Statistics, 0));
        assert_eq!(m.step(press(|k| k.start = true)), Action::Close);
    }

    #[test]
    fn rows_wrap_round() {
        let mut m = Menu::open(Page::Pause, 0);
        m.step(press(|k| k.up = true));
        assert_eq!(m.top(), (Page::Pause, PAUSE.len() - 1));
        assert_eq!(m.step(press(|k| k.a = true)), Action::SaveAndQuit);
    }

    #[test]
    fn deleting_a_journey_takes_two_presses_in_a_row() {
        let mut m = Menu::open(Page::Journeys, 1);
        assert_eq!(m.step(press(|k| k.x = true)), Action::None);
        assert!(m.confirm);
        assert_eq!(m.step(Keys::default()), Action::None);
        assert!(m.confirm, "waiting keeps the question open");
        assert_eq!(m.step(press(|k| k.x = true)), Action::Clear(1));
        assert!(!m.confirm);
        // Moving away cancels it.
        m.step(press(|k| k.x = true));
        m.step(press(|k| k.down = true));
        assert!(!m.confirm);
        assert_eq!(m.step(press(|k| k.x = true)), Action::None);
    }

    #[test]
    fn choosing_a_journey_loads_it_and_b_closes() {
        let mut m = Menu::open(Page::Journeys, 7);
        assert_eq!(m.top(), (Page::Journeys, crate::save::SLOTS - 1));
        assert_eq!(m.step(press(|k| k.a = true)), Action::Load(crate::save::SLOTS - 1));
        assert_eq!(m.step(press(|k| k.start = true)), Action::None, "start does not leave the journeys");
        assert_eq!(m.step(press(|k| k.b = true)), Action::Close);
    }
}
