//! Achievements: small milestones of a journey, shown in the journal and
//! kept in the journey's save by name.

use crate::save::Journey;
use crate::text::T;

/// One achievement: its name in saves, its title and how to earn it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ach {
    pub key: &'static str,
    pub name: T,
    pub hint: T,
}

const fn ach(key: &'static str, name: T, hint: T) -> Ach {
    Ach { key, name, hint }
}

pub const ALL: [Ach; 12] = [
    ach("sprout", T::AchSprout, T::AchSproutHint),
    ach("rose", T::AchRose, T::AchRoseHint),
    ach("watch", T::AchWatch, T::AchWatchHint),
    ach("fox", T::AchFox, T::AchFoxHint),
    ach("aviator", T::AchAviator, T::AchAviatorHint),
    ach("home", T::AchHome, T::AchHomeHint),
    ach("sunsets", T::AchSunsets, T::AchSunsetsHint),
    ach("stardust", T::AchStardust, T::AchStardustHint),
    ach("flights", T::AchFlights, T::AchFlightsHint),
    ach("troubles", T::AchTroubles, T::AchTroublesHint),
    ach("walker", T::AchWalker, T::AchWalkerHint),
    ach("jumps", T::AchJumps, T::AchJumpsHint),
];

/// The achievement saved as `key`.
pub fn index(key: &str) -> Option<usize> {
    ALL.iter().position(|a| a.key == key)
}

/// Whether journey `j` has earned achievement `i`.
fn earned(i: usize, j: &Journey) -> bool {
    let s = &j.stats;
    match ALL[i].key {
        "sprout" => s.sprouts >= 1,
        "rose" => j.done[0],
        "watch" => j.done[1],
        "fox" => j.parts[2][0] >= 5,
        "aviator" => j.parts[2][1] >= 4,
        "home" => j.finished,
        // As many as the prince once watched in a single day.
        "sunsets" => s.sunsets >= 44,
        "stardust" => j.stardust >= 50,
        "flights" => s.flights >= 10,
        "troubles" => s.troubles >= 20,
        "walker" => s.distance >= 1000.0,
        "jumps" => s.jumps >= 100,
        _ => false,
    }
}

/// Marks what `j` has newly earned and returns those achievements. Once
/// earned, an achievement stays, whatever happens after.
pub fn check(j: &mut Journey) -> Vec<usize> {
    let mut new = Vec::new();
    for i in 0..ALL.len() {
        if j.achievements & 1 << i == 0 && earned(i, j) {
            j.achievements |= 1 << i;
            new.push(i);
        }
    }
    new
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn achievements_are_earned_once_and_kept() {
        let mut j = Journey::start();
        assert!(check(&mut j).is_empty());
        j.stats.sprouts = 1;
        j.done[0] = true;
        assert_eq!(check(&mut j), vec![0, 1]);
        assert!(check(&mut j).is_empty(), "nothing new the second time");
        j.stardust = 60;
        assert_eq!(check(&mut j), vec![7]);
        j.stardust = 0;
        assert!(check(&mut j).is_empty());
        assert_ne!(j.achievements & 1 << 7, 0, "spending stardust keeps the achievement");
    }

    #[test]
    fn every_key_is_known_and_unique() {
        for (i, a) in ALL.iter().enumerate() {
            assert_eq!(index(a.key), Some(i));
        }
    }
}
