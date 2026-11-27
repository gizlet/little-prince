//! Button replays for `LITTLEPRINCE_SCRIPT=FILE`: the game plays the
//! presses in the file at a steady 60 steps a second and checks its state
//! as it goes, then quits with status 1 if a check failed.
//!
//! One command a line, after the second it happens at; `#` starts a comment:
//!
//! ```text
//! 1.0 press a           # press A for a tenth of a second
//! 2.0 press right 3     # hold right for three seconds
//! 5.5 check mode = play
//! 5.5 check jumps >= 1
//! 9.0 quit
//! ```
//!
//! `check NAME OP VALUE` compares a value the game reports (see
//! `App::probe`) with `=`, `!=`, `>=` or `<=`; numbers compare as numbers.

use crate::input::Button;

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Press(Button, f32),
    Check(String, Op, String),
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Eq,
    Ne,
    Ge,
    Le,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub at: f32,
    pub cmd: Cmd,
    /// The line number in the file, for reports.
    pub line: usize,
}

fn button(name: &str) -> Option<Button> {
    Some(match name.to_ascii_lowercase().as_str() {
        "up" => Button::Up,
        "down" => Button::Down,
        "left" => Button::Left,
        "right" => Button::Right,
        "a" => Button::A,
        "b" => Button::B,
        "x" => Button::X,
        "y" => Button::Y,
        "l" | "l1" => Button::L1,
        "r" | "r1" => Button::R1,
        "start" => Button::Start,
        "select" => Button::Select,
        _ => return None,
    })
}

/// Reads a script; an error names the first line it cannot read.
pub fn parse(text: &str) -> Result<Vec<Line>, String> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let bad = || format!("script line {}: cannot read {raw:?}", i + 1);
        let words: Vec<&str> = line.split_whitespace().collect();
        let at: f32 = words[0].parse().map_err(|_| bad())?;
        let cmd = match &words[1..] {
            ["press", b] => Cmd::Press(button(b).ok_or_else(bad)?, 0.1),
            ["press", b, secs] => Cmd::Press(button(b).ok_or_else(bad)?, secs.parse().map_err(|_| bad())?),
            ["check", name, op, value] => {
                let op = match *op {
                    "=" | "==" => Op::Eq,
                    "!=" => Op::Ne,
                    ">=" => Op::Ge,
                    "<=" => Op::Le,
                    _ => return Err(bad()),
                };
                Cmd::Check(name.to_string(), op, value.to_string())
            }
            ["quit"] => Cmd::Quit,
            _ => return Err(bad()),
        };
        out.push(Line { at, cmd, line: i + 1 });
    }
    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    Ok(out)
}

/// Whether `actual` passes `op value`: as numbers when `value` is one, else
/// as text. A number compared with something that is not one never passes.
pub fn holds(actual: &str, op: Op, value: &str) -> bool {
    match (actual.parse::<f64>(), value.parse::<f64>()) {
        (Ok(a), Ok(v)) => match op {
            Op::Eq => (a - v).abs() < 1e-6,
            Op::Ne => (a - v).abs() >= 1e-6,
            Op::Ge => a >= v,
            Op::Le => a <= v,
        },
        (Err(_), Ok(_)) => false,
        _ => match op {
            Op::Eq => actual == value,
            Op::Ne => actual != value,
            Op::Ge => actual >= value,
            Op::Le => actual <= value,
        },
    }
}

/// A script being played.
pub struct Player {
    lines: Vec<Line>,
    next: usize,
    /// Buttons held and until when.
    held: Vec<(Button, f32)>,
    pub failures: usize,
    pub checks: usize,
    pub done: bool,
}

impl Player {
    pub fn new(lines: Vec<Line>) -> Player {
        Player { lines, next: 0, held: Vec::new(), failures: 0, checks: 0, done: false }
    }

    /// Runs the commands due by `now`: `probe` reports a named value. Returns
    /// the buttons down this step.
    pub fn step(&mut self, now: f32, probe: &dyn Fn(&str) -> String) -> Vec<Button> {
        while let Some(l) = self.lines.get(self.next).filter(|l| l.at <= now) {
            match &l.cmd {
                Cmd::Press(b, secs) => self.held.push((*b, now + secs)),
                Cmd::Check(name, op, value) => {
                    self.checks += 1;
                    let actual = probe(name);
                    // A name the game does not know is a mistake in the script.
                    if !actual.starts_with("unknown ") && holds(&actual, *op, value) {
                        eprintln!("script: ok   line {}: {name} {op:?} {value} ({actual})", l.line);
                    } else {
                        self.failures += 1;
                        eprintln!("script: FAIL line {}: {name} {op:?} {value}, but it is {actual}", l.line);
                    }
                }
                Cmd::Quit => self.done = true,
            }
            self.next += 1;
        }
        self.held.retain(|(_, until)| *until > now);
        if self.next >= self.lines.len() && self.held.is_empty() {
            self.done = true;
        }
        self.held.iter().map(|(b, _)| *b).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_reads_presses_checks_and_comments() {
        let lines =
            parse("# a journey\n2 press right 3\n1.0 press A  # set off\n\n5 check jumps >= 2\n9 quit\n").unwrap();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], Line { at: 1.0, cmd: Cmd::Press(Button::A, 0.1), line: 3 });
        assert_eq!(lines[1].cmd, Cmd::Press(Button::Right, 3.0));
        assert_eq!(lines[2].cmd, Cmd::Check("jumps".into(), Op::Ge, "2".into()));
        assert_eq!(lines[3].cmd, Cmd::Quit);
        assert!(parse("1 press z").unwrap_err().contains("line 1"));
        assert!(parse("soon press a").is_err());
        assert!(parse("1 check jumps > 2").is_err());
    }

    #[test]
    fn checks_compare_numbers_as_numbers() {
        assert!(holds("10", Op::Ge, "9"));
        assert!(!holds("10", Op::Le, "9"));
        assert!(holds("3.0", Op::Eq, "3"));
        assert!(holds("play", Op::Eq, "play"));
        assert!(holds("title", Op::Ne, "play"));
        assert!(!holds("unknown jumpss", Op::Ge, "2"));
        assert!(!holds("none", Op::Ne, "1"));
    }

    #[test]
    fn checking_a_name_the_game_does_not_know_fails() {
        let mut p = Player::new(parse("0 check jumpss >= 2\n0 check mode != play\n").unwrap());
        p.step(0.0, &|name: &str| if name == "mode" { "title".into() } else { format!("unknown {name}") });
        assert_eq!((p.checks, p.failures), (2, 1));
    }

    #[test]
    fn buttons_stay_down_for_their_time() {
        let mut p = Player::new(parse("0 press a\n0.5 press right 1\n0.6 check mode = play").unwrap());
        let probe = |_: &str| "play".to_string();
        assert_eq!(p.step(0.0, &probe), vec![Button::A]);
        assert!(p.step(0.2, &probe).is_empty());
        assert_eq!(p.step(0.6, &probe), vec![Button::Right]);
        assert_eq!((p.checks, p.failures), (1, 0));
        assert!(!p.done);
        assert!(p.step(1.6, &probe).is_empty());
        assert!(p.done);
    }
}
