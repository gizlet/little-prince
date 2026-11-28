//! Conversations: a character's lines shown one at a time, each revealed a
//! few letters at a time. A shows the rest of a line or moves to the next;
//! B skips to the end. What a character says depends on how far its
//! planet's task has got, and finishing the right conversation completes it.

use crate::text::{s, Lang, T};
use crate::world::Kind;

/// Letters shown a second.
const PACE: f32 = 32.0;

/// What finishing a conversation does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Finish {
    Nothing,
    /// Completes the task of planet `n`.
    Complete(usize),
    /// Begins the ending.
    Ending,
}

#[derive(Clone, Debug)]
pub struct Dialog {
    pub speaker: T,
    pub lines: Vec<T>,
    pub line: usize,
    /// Letters of the current line shown so far.
    pub shown: f32,
    pub finish: Finish,
}

impl Dialog {
    /// What the character of planet `planet` says: once its task is `done`,
    /// when the task is `ready` to finish, or before.
    pub fn with(kind: Kind, planet: usize, done: bool, ready: bool) -> Dialog {
        let (speaker, intro, finished, idle) = match kind {
            Kind::B612 => (T::Rose, [T::RoseIntro1, T::RoseIntro2], [T::RoseDone1, T::RoseDone2], T::RoseIdle),
            Kind::Lamp => {
                (T::Keeper, [T::KeeperIntro1, T::KeeperIntro2], [T::KeeperDone1, T::KeeperDone2], T::KeeperIdle)
            }
            Kind::Desert => (T::Fox, [T::FoxIntro1, T::FoxIntro2], [T::FoxDone1, T::FoxDone2], T::FoxIdle),
        };
        let (lines, finish) = if done {
            (vec![idle], Finish::Nothing)
        } else if ready {
            (finished.to_vec(), Finish::Complete(planet))
        } else {
            (intro.to_vec(), Finish::Nothing)
        };
        Dialog { speaker, lines, line: 0, shown: 0.0, finish }
    }

    /// The rose, when the prince comes home with every task done.
    pub fn homecoming() -> Dialog {
        Dialog { speaker: T::Rose, lines: vec![T::RoseEnd1, T::RoseEnd2], line: 0, shown: 0.0, finish: Finish::Ending }
    }

    pub fn update(&mut self, dt: f32) {
        self.shown += dt * PACE;
    }

    fn len(&self, l: Lang) -> usize {
        self.lines.get(self.line).map_or(0, |t| s(*t, l).chars().count())
    }

    /// Whether the current line is shown in full.
    pub fn whole(&self, l: Lang) -> bool {
        self.shown as usize >= self.len(l)
    }

    /// A: shows the rest of the line, or moves to the next; true once past
    /// the last.
    pub fn press(&mut self, l: Lang) -> bool {
        if !self.whole(l) {
            self.shown = self.len(l) as f32;
            return false;
        }
        self.line += 1;
        self.shown = 0.0;
        self.line >= self.lines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_says_what_the_task_calls_for() {
        assert_eq!(Dialog::with(Kind::B612, 0, false, false).lines, vec![T::RoseIntro1, T::RoseIntro2]);
        assert_eq!(Dialog::with(Kind::B612, 0, false, false).finish, Finish::Nothing);
        assert_eq!(Dialog::with(Kind::Lamp, 1, false, true).finish, Finish::Complete(1));
        let after = Dialog::with(Kind::Desert, 2, true, true);
        assert_eq!((after.lines, after.finish), (vec![T::FoxIdle], Finish::Nothing));
    }

    #[test]
    fn a_shows_the_rest_of_a_line_then_moves_on() {
        let mut d = Dialog::with(Kind::B612, 0, false, true);
        d.update(0.1);
        assert!(!d.whole(Lang::En));
        assert!(!d.press(Lang::En));
        assert!(d.whole(Lang::En));
        assert_eq!(d.line, 0);
        assert!(!d.press(Lang::En));
        assert_eq!((d.line, d.shown), (1, 0.0));
        d.update(10.0);
        assert!(d.press(Lang::En), "past the last line");
    }
}
