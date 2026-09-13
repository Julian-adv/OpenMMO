//! Draw true deeds from the ledger for the bard to embellish and sing.

use std::collections::HashMap;

use rand::seq::SliceRandom;
use rand::Rng;
use tracing::warn;

pub use onlinerpg_shared::tales::Deed;

pub const LEDGER_PATH: &str = "data/tales/ledger.txt";

/// Deeds drawn for one performance set.
pub const PICKS_PER_SET: usize = 3;

/// Read the ledger, oldest first. A missing file is an empty ledger.
pub fn load_ledger(path: &str) -> Vec<Deed> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_ledger(&content)
}

pub fn parse_ledger(content: &str) -> Vec<Deed> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .filter_map(|l| {
            let deed = Deed::parse(l);
            if deed.is_none() {
                warn!("Skipping malformed tale line: {l}");
            }
            deed
        })
        .collect()
}

/// Plain songs between one tale and the next, so late arrivals still hear
/// one and the set is not all stories.
pub const SONGS_BETWEEN_TALES: usize = 2;

/// One performance set: a few deeds, one per hero, newer lines favoured, sung in
/// turn and round again, a couple of songs apart.
#[derive(Debug, Default)]
pub struct SetTales {
    picks: Vec<Deed>,
    /// Tales told so far in this set: picks the current one and paces the
    /// language alternation.
    sung: usize,
    /// Our song count at which the next tale is due; 0 until the first.
    next_tale_at: usize,
}

impl SetTales {
    pub fn draw<R: Rng>(ledger: &[Deed], count: usize, rng: &mut R) -> SetTales {
        // Rank weight: the newest line is worth `len` times the oldest.
        let mut pool: Vec<(usize, &Deed)> = ledger.iter().enumerate().collect();
        let mut picks = Vec::new();
        while picks.len() < count && !pool.is_empty() {
            let Ok(&(_, deed)) = pool.choose_weighted(rng, |(i, _)| (*i + 1) as f64) else {
                break;
            };
            let deed = deed.clone();
            pool.retain(|(_, d)| d.hero != deed.hero);
            picks.push(deed);
        }
        SetTales {
            picks,
            ..Default::default()
        }
    }

    pub fn len(&self) -> usize {
        self.picks.len()
    }

    pub fn current(&self) -> Option<&Deed> {
        self.picks.get(self.sung % self.picks.len().max(1))
    }

    pub fn is_due(&self, songs_started: usize) -> bool {
        self.current().is_some() && songs_started >= self.next_tale_at
    }

    /// Told: the next one waits for this tale's own song plus the gap.
    pub fn advance(&mut self, songs_started: usize) {
        self.sung += 1;
        self.next_tale_at = songs_started + SONGS_BETWEEN_TALES + 1;
    }

    pub fn sung(&self) -> usize {
        self.sung
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Korean,
    English,
}

impl Lang {
    fn name(self) -> &'static str {
        match self {
            Lang::Korean => "Korean",
            Lang::English => "English",
        }
    }
}

fn lang_of_text(text: &str) -> Option<Lang> {
    let hangul = text
        .chars()
        .filter(|c| ('\u{AC00}'..='\u{D7A3}').contains(c))
        .count();
    let latin = text.chars().filter(char::is_ascii_alphabetic).count();
    match hangul.cmp(&latin) {
        std::cmp::Ordering::Greater => Some(Lang::Korean),
        std::cmp::Ordering::Less => Some(Lang::English),
        std::cmp::Ordering::Equal => None,
    }
}

/// What the room speaks, read off the conversation history's `[Chat]`
/// lines (players, not NPCs, not ourselves): `Some` only when every
/// speaker used the same language, `None` for a mixed or silent room.
pub fn audience_lang<'a>(
    history: impl IntoIterator<Item = &'a String>,
    self_name: &str,
) -> Option<Lang> {
    let mut speakers: HashMap<&str, (usize, usize)> = HashMap::new();
    for line in history {
        let Some(rest) = line.split("[Chat] ").nth(1) else {
            continue;
        };
        let Some((name, text)) = rest.split_once(": ") else {
            continue;
        };
        if name == self_name {
            continue;
        }
        let tally = speakers.entry(name).or_default();
        match lang_of_text(text) {
            Some(Lang::Korean) => tally.0 += 1,
            Some(Lang::English) => tally.1 += 1,
            None => {}
        }
    }
    let mut room: Option<Lang> = None;
    for (ko, en) in speakers.values() {
        let lang = match ko.cmp(en) {
            std::cmp::Ordering::Greater => Lang::Korean,
            std::cmp::Ordering::Less => Lang::English,
            std::cmp::Ordering::Equal => continue,
        };
        match room {
            None => room = Some(lang),
            Some(l) if l == lang => {}
            Some(_) => return None,
        }
    }
    room
}

/// The language of the `nth` tale in the set: the room's, when it is of one
/// mind; otherwise Korean and English turn about, Korean first.
pub fn tale_lang(audience: Option<Lang>, nth: usize) -> Lang {
    audience.unwrap_or(if nth.is_multiple_of(2) {
        Lang::Korean
    } else {
        Lang::English
    })
}

/// Prompt section carrying the one deed the bard sings next; the how is
/// in bard.txt's Tales rules.
pub fn prompt_section(deed: &Deed, lang: Lang, automatic_due: bool) -> String {
    let rotation = if automatic_due { "DUE" } else { "WAITING" };
    format!(
        "\n=== CURRENT TALE (a true deed — sing it as directed) ===\nAutomatic rotation: {rotation}\n\
         Date: {}\nHero: {}\n\
         Facts and performance direction: {}\n\
         Language: {} for the opening line and every verse, whatever the listeners spoke.\n",
        deed.date,
        deed.hero,
        deed.brief,
        lang.name()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    const LEDGER: &str = "\
# comment
2026-09-01 | Alder | Alder slew the Ogre Warlord. Celebrate the realm's first victory over it.
2026-09-01 | Brann | Brann's steel longsword shattered at +9. Sing it as a tragedy.
2026-09-02 | Cyra | Cyra reached level 32, surpassing Alder's level 31 record.
2026-09-02 | Alder | Alder climbed six levels in one day and reached level 14.
2026-09-02 | Dov | Dov earned more experience than anyone for a third day running.
2026-09-02 | Eir | Eir travelled farther from Aldermark than anyone, reaching Brovik.
2026-09-02 | Fenn | Fenn danced in the rain | keep this phrase intact.
garbage
2026-09-04 | Gorm | Gorm abused a respawn reset to climb from level 20 to 25. Satirize only the verified trick.
";

    #[test]
    fn pipe_delimited_lines_are_trimmed_and_bad_lines_are_skipped() {
        let deeds = parse_ledger(LEDGER);
        assert_eq!(deeds.len(), 8, "{deeds:?}");
        assert_eq!(deeds[0].date, "2026-09-01");
        assert_eq!(deeds[0].hero, "Alder");
        assert!(deeds[0].brief.contains("first victory"));
        assert!(deeds[6].brief.ends_with("| keep this phrase intact."));
        assert!(Deed::parse("2026-09-01 | | missing hero").is_none());
        assert!(Deed::parse("2026-09-01 | Alder | ").is_none());
    }

    #[test]
    fn the_prompt_passes_the_natural_language_brief_through_unchanged() {
        let brief = "판사가 레벨 34에 도달했다. 과거 어뷰즈를 가볍게 꼬집되 현재 성취는 인정한다.";
        let deed = Deed::parse(&format!("2026-09-07 | 판사 | {brief}")).unwrap();
        let prompt = prompt_section(&deed, Lang::Korean, true);
        assert!(prompt.contains("Date: 2026-09-07"), "{prompt}");
        assert!(prompt.contains("Hero: 판사"), "{prompt}");
        assert!(prompt.contains(&format!("Facts and performance direction: {brief}")));
        assert!(prompt.contains("Language: Korean"), "{prompt}");
        assert!(!prompt.contains("Mood:"), "{prompt}");

        let waiting = prompt_section(&deed, Lang::Korean, false);
        assert!(waiting.contains("Automatic rotation: WAITING"), "{waiting}");
    }

    #[test]
    fn a_draw_takes_one_deed_per_hero_up_to_the_cap() {
        let deeds = parse_ledger(LEDGER);
        let mut rng = StdRng::seed_from_u64(7);
        let set = SetTales::draw(&deeds, 3, &mut rng);
        assert_eq!(set.picks.len(), 3);
        let mut names: Vec<&str> = set.picks.iter().map(|d| d.hero.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 3, "{:?}", set.picks);

        let short = SetTales::draw(&deeds[..1], 3, &mut rng);
        assert_eq!(short.picks.len(), 1);
        assert!(SetTales::draw(&[], 3, &mut rng).current().is_none());
    }

    #[test]
    fn newer_lines_are_favoured() {
        let deeds = parse_ledger(LEDGER);
        let mut rng = StdRng::seed_from_u64(1);
        let mut first_pick_is_old = 0;
        for _ in 0..200 {
            let set = SetTales::draw(&deeds, 1, &mut rng);
            if set.picks[0].date == "2026-09-01" {
                first_pick_is_old += 1;
            }
        }
        assert!(
            first_pick_is_old < 60,
            "{first_pick_is_old} of 200 picks were old"
        );
    }

    #[test]
    fn the_room_decides_the_language_and_a_mixed_room_alternates() {
        let ko = |n: &str, t: &str| format!("[19:02] [Chat] {n}: {t}");
        let all_korean = vec![
            ko("민수", "노래 좋네요"),
            ko("jake1", "한 곡 더 부탁해요"),
            "[19:03] [NpcChat] Cocoly: Welcome, traveller!".to_string(),
            ko("Signe", "This one is First Light Waltz"),
        ];
        assert_eq!(audience_lang(&all_korean, "Signe"), Some(Lang::Korean));
        assert_eq!(tale_lang(Some(Lang::Korean), 1), Lang::Korean);

        let all_english = vec![ko("Ann", "play something sad"), ko("Bob", "cheers!")];
        assert_eq!(audience_lang(&all_english, "Signe"), Some(Lang::English));
        assert_eq!(tale_lang(Some(Lang::English), 0), Lang::English);

        let mixed = vec![ko("민수", "노래 좋네요"), ko("Ann", "play something sad")];
        assert_eq!(audience_lang(&mixed, "Signe"), None);
        assert_eq!(tale_lang(None, 0), Lang::Korean);
        assert_eq!(tale_lang(None, 1), Lang::English);
        assert_eq!(tale_lang(None, 2), Lang::Korean);

        let silent: Vec<String> = vec![ko("Signe", "이번에는 First Light Waltz")];
        assert_eq!(
            audience_lang(&silent, "Signe"),
            None,
            "our own lines say nothing"
        );
        assert_eq!(audience_lang(&[ko("Ann", "123 !!")], "Signe"), None);
    }

    #[test]
    fn the_set_rotates_and_wraps() {
        let deeds = parse_ledger(LEDGER);
        let mut rng = StdRng::seed_from_u64(3);
        let mut set = SetTales::draw(&deeds, 2, &mut rng);
        let first = set.current().cloned().unwrap();
        assert_eq!(set.sung(), 0);
        set.advance(0);
        assert_eq!(set.sung(), 1);
        assert_ne!(set.current(), Some(&first));
        set.advance(0);
        assert_eq!(set.current(), Some(&first));
        let mut empty = SetTales::default();
        empty.advance(0);
        assert!(empty.current().is_none());
    }

    /// A tale, its song, two plain songs, the next tale.
    #[test]
    fn tales_come_two_songs_apart() {
        let deeds = parse_ledger(LEDGER);
        let mut rng = StdRng::seed_from_u64(3);
        let mut set = SetTales::draw(&deeds, 2, &mut rng);
        assert!(set.is_due(5), "the first tale waits for nothing");
        set.advance(5);
        assert!(!set.is_due(6), "the tale's own song");
        assert!(!set.is_due(7), "one plain song");
        assert!(set.is_due(8), "two plain songs");
        assert!(!SetTales::default().is_due(9));
    }
}
