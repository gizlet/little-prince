//! Saved journeys and settings, as plain text lines in `GIZLET_SAVE_DIR`
//! (`./save` when run outside Gizlet OS).
//!
//! - `slot1.txt` to `slot3.txt`: one journey each, where the traveller is,
//!   what was done on each planet and the statistics.
//! - `settings.txt`: language, effects, sound and camera, shared by all slots.
//!
//! Each file starts with a header naming it and its version. Lines are
//! `key value`; lines a version does not know are kept and written back, so a
//! journey saved by a newer version survives being played on an older one.
//! Files are written to a temporary name and renamed into place, so a power
//! cut never leaves half a save.

use std::path::PathBuf;

/// The version these files are written as.
/// Version 2 added each planet's task parts and stardust; a version 1
/// journey's score becomes the first part.
pub const VERSION: u32 = 2;
/// How many journeys can be kept.
pub const SLOTS: usize = 3;
/// The planets' names in save files, in `world::KINDS` order.
pub const PLANETS: [&str; 3] = ["b612", "lamp", "desert"];

const SLOT_HEADER: &str = "littleprince-journey";
const SETTINGS_HEADER: &str = "littleprince-settings";

/// What a save slot holds.
#[derive(Clone, PartialEq, Debug)]
pub enum Slot {
    Empty,
    Journey(Journey),
    /// A file that cannot be read or is not a journey: kept as it is, never
    /// replaced by a new journey unless it is deleted first.
    Unreadable,
}

/// Running totals for the statistics page.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Stats {
    /// Seconds played.
    pub time: f64,
    /// World units walked.
    pub distance: f64,
    pub jumps: u32,
    pub sunsets: u32,
    pub sprouts: u32,
    pub flights: u32,
    /// Troubles seen to: volcanoes swept, caterpillars turned butterflies.
    pub troubles: u32,
    /// Times a trouble knocked the prince back.
    pub bumps: u32,
}

/// One journey.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Journey {
    /// The planet the traveller is on, an index into `PLANETS`.
    pub planet: usize,
    pub visited: [bool; 3],
    /// The planets whose task is done: it opens the route to the next.
    pub done: [bool; 3],
    /// Each planet's score: sprouts pulled on B612, sunsets watched on the
    /// lamplighter's planet, the fox's trust in the desert.
    pub scores: [usize; 3],
    /// Each planet's task in parts (B612: sprouts, volcanoes, caterpillars).
    pub parts: [[usize; 3]; 3],
    /// B612's active volcanoes swept, bit `i` for the `i`th; `None` in
    /// saves from before it was kept.
    pub swept: Option<u8>,
    /// Stardust carried.
    pub stardust: u32,
    pub stats: Stats,
    /// Lines this version does not understand, kept as they were.
    extra: Vec<String>,
    /// The version the file was written as.
    pub version: u32,
}

impl Journey {
    /// A journey just begun on B612.
    pub fn start() -> Journey {
        Journey { visited: [true, false, false], version: VERSION, ..Journey::default() }
    }

    /// Reads a journey; `None` when the text is not one.
    pub fn parse(text: &str) -> Option<Journey> {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        let version = match lines.next()?.split_once(' ') {
            Some((SLOT_HEADER, v)) => v.trim().parse().ok()?,
            _ => return None,
        };
        let mut j = Journey { version, ..Journey::default() };
        let mut has_parts = [false; 3];
        for line in lines {
            let (k, v) = line.split_once(' ').map_or((line, ""), |(k, v)| (k, v.trim()));
            let known = match k {
                "planet" => PLANETS.iter().position(|p| *p == v).map(|i| j.planet = i).is_some(),
                "visited" => {
                    for name in v.split_whitespace() {
                        if let Some(i) = PLANETS.iter().position(|p| *p == name) {
                            j.visited[i] = true;
                        }
                    }
                    true
                }
                "done" => {
                    for name in v.split_whitespace() {
                        if let Some(i) = PLANETS.iter().position(|p| *p == name) {
                            j.done[i] = true;
                        }
                    }
                    true
                }
                "time" => v.parse().map(|x| j.stats.time = x).is_ok(),
                "distance" => v.parse().map(|x| j.stats.distance = x).is_ok(),
                "jumps" => v.parse().map(|x| j.stats.jumps = x).is_ok(),
                "sunsets" => v.parse().map(|x| j.stats.sunsets = x).is_ok(),
                "sprouts" => v.parse().map(|x| j.stats.sprouts = x).is_ok(),
                "flights" => v.parse().map(|x| j.stats.flights = x).is_ok(),
                "troubles" => v.parse().map(|x| j.stats.troubles = x).is_ok(),
                "bumps" => v.parse().map(|x| j.stats.bumps = x).is_ok(),
                "stardust" => v.parse().map(|x| j.stardust = x).is_ok(),
                "swept" => {
                    let list: Option<Vec<u8>> =
                        v.split_whitespace().map(|x| x.parse().ok().filter(|i: &u8| *i < 8)).collect();
                    list.map(|l| j.swept = Some(l.iter().fold(0, |m, i| m | 1 << i))).is_some()
                }
                _ => match k.split_once('.').and_then(|(p, key)| Some((PLANETS.iter().position(|n| *n == p)?, key))) {
                    Some((i, "score")) => v.parse().map(|x| j.scores[i] = x).is_ok(),
                    Some((i, "parts")) => {
                        let nums: Vec<usize> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                        for (part, n) in j.parts[i].iter_mut().zip(&nums) {
                            *part = *n;
                        }
                        has_parts[i] = !nums.is_empty();
                        has_parts[i]
                    }
                    _ => false,
                },
            };
            if !known {
                j.extra.push(line.to_string());
            }
        }
        j.visited[j.planet] = true;
        // Before version 2 a planet's score was its only part.
        for i in 0..3 {
            if !has_parts[i] {
                j.parts[i] = [j.scores[i], 0, 0];
            }
        }
        Some(j)
    }

    pub fn render(&self) -> String {
        let visited: Vec<&str> = PLANETS.iter().zip(self.visited).filter(|(_, v)| *v).map(|(p, _)| *p).collect();
        let s = &self.stats;
        let mut out = format!(
            "{SLOT_HEADER} {}\nplanet {}\nvisited {}\n",
            self.version.max(VERSION),
            PLANETS[self.planet],
            visited.join(" ")
        );
        let done: Vec<&str> = PLANETS.iter().zip(self.done).filter(|(_, d)| *d).map(|(p, _)| *p).collect();
        out += &format!("done {}\n", done.join(" "));
        for (p, (score, parts)) in PLANETS.iter().zip(self.scores.iter().zip(self.parts)) {
            out += &format!("{p}.score {score}\n{p}.parts {} {} {}\n", parts[0], parts[1], parts[2]);
        }
        out += &format!("stardust {}\ntroubles {}\nbumps {}\n", self.stardust, s.troubles, s.bumps);
        if let Some(swept) = self.swept {
            let list: Vec<String> = (0..8).filter(|i| swept & 1 << i != 0).map(|i| i.to_string()).collect();
            out += &format!("swept {}\n", list.join(" "));
        }
        out += &format!(
            "time {:.1}\ndistance {:.1}\njumps {}\nsunsets {}\nsprouts {}\nflights {}\n",
            s.time, s.distance, s.jumps, s.sunsets, s.sprouts, s.flights
        );
        for line in &self.extra {
            out += line;
            out.push('\n');
        }
        out
    }
}

/// Settings shared by all journeys. `None` leaves a choice to the platform
/// default or the environment.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Settings {
    /// `zh` or `en`.
    pub lang: Option<String>,
    /// Effects level, 0 to 2.
    pub fx: Option<u8>,
    /// Whether the performance panel shows.
    pub stats: Option<bool>,
    /// Sound volume, 0 to 10.
    pub volume: u8,
    /// Whether pushing the camera stick up looks down.
    pub invert: bool,
    /// The journey played last, 0-based.
    pub slot: usize,
    extra: Vec<String>,
}

impl Settings {
    /// The settings before any are chosen.
    pub fn fresh() -> Settings {
        Settings { volume: 8, ..Settings::default() }
    }

    pub fn parse(text: &str) -> Option<Settings> {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        match lines.next()?.split_once(' ') {
            Some((SETTINGS_HEADER, v)) => v.trim().parse::<u32>().ok()?,
            _ => return None,
        };
        let mut s = Settings::fresh();
        for line in lines {
            let (k, v) = line.split_once(' ').map_or((line, ""), |(k, v)| (k, v.trim()));
            let known = match k {
                "lang" if v == "zh" || v == "en" => {
                    s.lang = Some(v.to_string());
                    true
                }
                "fx" => v.parse().ok().filter(|x| *x <= 2).map(|x| s.fx = Some(x)).is_some(),
                "stats" => v.parse::<u8>().map(|x| s.stats = Some(x != 0)).is_ok(),
                "volume" => v.parse::<u8>().map(|x| s.volume = x.min(10)).is_ok(),
                "invert" => v.parse::<u8>().map(|x| s.invert = x != 0).is_ok(),
                "slot" => v.parse::<usize>().ok().filter(|x| *x < SLOTS).map(|x| s.slot = x).is_some(),
                _ => false,
            };
            if !known {
                s.extra.push(line.to_string());
            }
        }
        Some(s)
    }

    pub fn render(&self) -> String {
        let mut out = format!("{SETTINGS_HEADER} {VERSION}\n");
        if let Some(l) = &self.lang {
            out += &format!("lang {l}\n");
        }
        if let Some(fx) = self.fx {
            out += &format!("fx {fx}\n");
        }
        if let Some(st) = self.stats {
            out += &format!("stats {}\n", st as u8);
        }
        out += &format!("volume {}\ninvert {}\nslot {}\n", self.volume, self.invert as u8, self.slot);
        for line in &self.extra {
            out += line;
            out.push('\n');
        }
        out
    }
}

/// Where saves are kept; writes nothing when `dir` is `None` (captures and
/// tests of the game itself).
pub struct Store {
    dir: Option<PathBuf>,
    /// The last write failed.
    pub failed: bool,
}

impl Store {
    pub fn from_env(enabled: bool) -> Store {
        let dir = std::env::var_os("GIZLET_SAVE_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("save"));
        Store { dir: enabled.then_some(dir), failed: false }
    }

    #[cfg(test)]
    pub fn at(dir: &std::path::Path) -> Store {
        Store { dir: Some(dir.to_path_buf()), failed: false }
    }

    fn slot_name(i: usize) -> String {
        format!("slot{}.txt", i + 1)
    }

    fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.as_ref()?.join(name)).ok()
    }

    /// Writes `text` to a temporary file next to `name`, then renames it;
    /// whether it was written (or there is nowhere to write).
    fn write(&mut self, name: &str, text: &str) -> bool {
        let Some(dir) = &self.dir else { return true };
        let ok = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(dir)?;
            let tmp = dir.join(format!("{name}.tmp"));
            {
                use std::io::Write;
                let mut f = std::fs::File::create(&tmp)?;
                f.write_all(text.as_bytes())?;
                f.sync_all()?;
            }
            std::fs::rename(&tmp, dir.join(name))?;
            // The rename only survives a power cut once the folder is synced.
            std::fs::File::open(dir)?.sync_all()
        })();
        if let Err(e) = &ok {
            eprintln!("littleprince: could not save {name}: {e}");
        }
        self.failed = ok.is_err();
        !self.failed
    }

    /// What is in a slot: nothing, a journey, or a file that cannot be read
    /// or is not a journey, which must not be overwritten by a new one.
    pub fn slot(&self, slot: usize) -> Slot {
        let Some(dir) = &self.dir else { return Slot::Empty };
        match std::fs::read_to_string(dir.join(Self::slot_name(slot))) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Slot::Empty,
            Err(_) => Slot::Unreadable,
            Ok(text) => Journey::parse(&text).map_or(Slot::Unreadable, Slot::Journey),
        }
    }

    pub fn journey(&self, slot: usize) -> Option<Journey> {
        match self.slot(slot) {
            Slot::Journey(j) => Some(j),
            _ => None,
        }
    }

    pub fn save_journey(&mut self, slot: usize, j: &Journey) -> bool {
        self.write(&Self::slot_name(slot), &j.render())
    }

    /// Moves a journey out of the way rather than deleting it: renamed to
    /// `slotN.txt.old`, replacing any older one. Whether the slot is empty
    /// now.
    pub fn clear_journey(&mut self, slot: usize) -> bool {
        let Some(dir) = &self.dir else { return true };
        let name = Self::slot_name(slot);
        let ok = match std::fs::rename(dir.join(&name), dir.join(format!("{name}.old"))) {
            Ok(()) => std::fs::File::open(dir).and_then(|d| d.sync_all()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        };
        if let Err(e) = &ok {
            eprintln!("littleprince: could not clear {name}: {e}");
        }
        self.failed = ok.is_err();
        !self.failed
    }

    pub fn settings(&self) -> Settings {
        self.read("settings.txt").and_then(|t| Settings::parse(&t)).unwrap_or_else(Settings::fresh)
    }

    pub fn save_settings(&mut self, s: &Settings) {
        self.write("settings.txt", &s.render());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Journey {
        let mut j = Journey::start();
        j.planet = 1;
        j.visited = [true, true, false];
        j.done = [true, false, false];
        j.scores = [5, 12, 0];
        j.parts = [[3, 1, 1], [12, 0, 0], [0, 0, 0]];
        j.stardust = 17;
        j.swept = Some(0b10);
        j.stats = Stats {
            time: 321.5,
            distance: 1234.5,
            jumps: 40,
            sunsets: 13,
            sprouts: 9,
            flights: 2,
            troubles: 4,
            bumps: 2,
        };
        j
    }

    #[test]
    fn a_version_1_score_becomes_the_first_part() {
        let old = format!("{SLOT_HEADER} 1\nplanet lamp\ndone b612\nb612.score 7\nlamp.score 2\n");
        let j = Journey::parse(&old).unwrap();
        assert_eq!(j.parts, [[7, 0, 0], [2, 0, 0], [0, 0, 0]]);
        assert_eq!((j.stardust, j.done[0]), (0, true));
        let again = Journey::parse(&j.render()).unwrap();
        assert_eq!(again.parts, j.parts);
        assert!(j.render().starts_with(&format!("{SLOT_HEADER} {VERSION}\n")));
    }

    #[test]
    fn a_journey_reads_back_as_it_was_written() {
        let j = sample();
        assert_eq!(Journey::parse(&j.render()), Some(j));
    }

    #[test]
    fn settings_read_back_as_they_were_written() {
        let s = Settings {
            lang: Some("en".into()),
            fx: Some(1),
            stats: Some(false),
            volume: 3,
            invert: true,
            slot: 2,
            extra: vec![],
        };
        assert_eq!(Settings::parse(&s.render()), Some(s));
        assert_eq!(Settings::parse(&Settings::fresh().render()), Some(Settings::fresh()));
    }

    #[test]
    fn a_newer_journey_keeps_what_this_version_does_not_know() {
        let newer = format!("{SLOT_HEADER} 7\nplanet desert\nhero knight\nb612.score 3\nstardust 120\n");
        let j = Journey::parse(&newer).unwrap();
        assert_eq!((j.planet, j.scores[0], j.version), (2, 3, 7));
        let again = j.render();
        assert!(again.starts_with(&format!("{SLOT_HEADER} 7\n")), "{again}");
        assert!(again.contains("hero knight\n") && again.contains("stardust 120\n"), "{again}");
        assert_eq!(Journey::parse(&again), Some(j));
    }

    #[test]
    fn an_older_journey_fills_in_what_it_lacks() {
        let j = Journey::parse(&format!("{SLOT_HEADER} 0\nplanet lamp\n")).unwrap();
        assert_eq!(j.planet, 1);
        assert_eq!(j.visited, [false, true, false]);
        assert_eq!(j.stats, Stats::default());
        assert!(j.render().starts_with(&format!("{SLOT_HEADER} {VERSION}\n")));
    }

    #[test]
    fn other_files_and_nonsense_are_not_journeys() {
        assert_eq!(Journey::parse(""), None);
        assert_eq!(Journey::parse("meals 3\nbest 10\n"), None);
        assert_eq!(Journey::parse(&format!("{SETTINGS_HEADER} 1\n")), None);
        let j = Journey::parse(&format!("{SLOT_HEADER} 1\nplanet mars\njumps many\n")).unwrap();
        assert_eq!((j.planet, j.stats.jumps), (0, 0));
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("littleprince-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_store_writes_whole_files_and_moves_cleared_journeys_aside() {
        let dir = temp_dir("store");
        let mut store = Store::at(&dir);
        assert_eq!(store.journey(0), None);
        let j = sample();
        store.save_journey(0, &j);
        assert!(!store.failed);
        assert_eq!(store.journey(0), Some(j.clone()));
        assert!(!dir.join("slot1.txt.tmp").exists());
        store.clear_journey(0);
        assert_eq!(store.journey(0), None);
        assert!(dir.join("slot1.txt.old").exists());
        let mut s = store.settings();
        s.slot = 1;
        store.save_settings(&s);
        assert_eq!(store.settings().slot, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_leaves_the_last_save_alone() {
        let dir = temp_dir("fail");
        let mut store = Store::at(&dir);
        let j = sample();
        store.save_journey(1, &j);
        // A directory where the temporary file would go makes the write fail.
        std::fs::create_dir_all(dir.join("slot2.txt.tmp")).unwrap();
        let mut other = j.clone();
        other.planet = 2;
        assert!(!store.save_journey(1, &other));
        assert!(store.failed);
        assert_eq!(store.journey(1), Some(j));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_saves_are_not_empty_slots() {
        let dir = temp_dir("unreadable");
        let store = Store::at(&dir);
        assert_eq!(store.slot(0), Slot::Empty);
        // A folder where the journey should be cannot be read as one.
        std::fs::create_dir_all(dir.join("slot1.txt")).unwrap();
        assert_eq!(store.slot(0), Slot::Unreadable);
        // Nor can a file that is not a journey.
        std::fs::write(dir.join("slot2.txt"), "meals 3\n").unwrap();
        assert_eq!(store.slot(1), Slot::Unreadable);
        assert_eq!(store.journey(1), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_deletion_keeps_the_journey_and_says_so() {
        let dir = temp_dir("clear");
        let mut store = Store::at(&dir);
        let j = sample();
        assert!(store.save_journey(2, &j));
        // A non-empty folder where the journey would be moved makes the
        // rename fail.
        std::fs::create_dir_all(dir.join("slot3.txt.old").join("x")).unwrap();
        assert!(!store.clear_journey(2));
        assert!(store.failed);
        assert_eq!(store.journey(2), Some(j));
        // An empty slot is already cleared.
        assert!(store.clear_journey(0));
        assert!(!store.failed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_store_without_a_folder_writes_nothing() {
        let mut store = Store { dir: None, failed: false };
        store.save_journey(0, &sample());
        assert_eq!(store.journey(0), None);
        assert!(!store.failed);
    }
}
