//! Spelling repair for Hungarian: a word that does not exist becomes the one
//! existing word that sounds the same.
//!
//! The speech model sometimes spells a word as it heard it ("munició",
//! "folyot"). A word is changed only when it is lower case, has four letters
//! or more, is unknown to the Hungarian dictionary, and exactly one known word
//! is reached by one spelling step (`candidates`), by a step that measured as
//! right on read speech (`STEPS`; `docs/bench/polish-09.md`). Names, known words
//! and the user's own dictionary words are never touched.
//!
//! Dictionary: Magyar Ispell 1.9 (Németh László, Godó Ferenc), MPL-2.0,
//! converted by `scripts/make-spell-dict.py`. It is unpacked on first use (about
//! 40 ms and 22 MB) and freed with the engine.

use std::collections::BTreeMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

static AFF: &[u8] = include_bytes!("../data/hu.aff.gz");
static DIC: &[u8] = include_bytes!("../data/hu.dic.gz");
static DICTIONARY: Mutex<Option<Arc<spellbook::Dictionary>>> = Mutex::new(None);

const MIN_LETTERS: usize = 4;
const VOWELS: &str = "aáeéiíoóöőuúüű";
const SOUND_PAIRS: &[(&str, &str)] = &[
    ("ly", "j"), ("j", "ly"), ("ty", "tj"), ("tj", "ty"), ("gy", "dj"), ("gy", "gyj"), ("ny", "nj"), ("nj", "ny"),
    ("ccs", "tcs"), ("cs", "ts"), ("cc", "tsz"), ("cc", "dsz"), ("cc", "tc"), ("ssz", "zsz"), ("tt", "dt"), ("dt", "tt"),
    ("gg", "kg"), ("kk", "gk"), ("ss", "zss"), ("ny", "gyn"), ("nny", "nnyj"), ("lly", "llj"), ("ggy", "dgy"),
    ("tty", "dty"), ("tty", "ttj"), ("ggy", "gyj"), ("ccs", "dcs"), ("pt", "bt"), ("bb", "pb"), ("szt", "zt"),
    ("kt", "gt"), ("ks", "x"), ("x", "ksz"), ("ksz", "x"), ("cs", "dzs"), ("nyj", "nny"),
];

/// One spelling step from the heard form to a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Pair(&'static str, &'static str),
    /// The vowel in the heard word, whose length or rounding changes.
    Vowel(char),
    Double(char),
    Single(char),
}

/// Steps that were right at least four times out of five on one part of the
/// speakers; the other part confirmed them (13 better, 0 worse of 4,495 clips).
const STEPS: &[Step] = &[
    Step::Pair("ly", "j"),
    Step::Vowel('ó'),
    Step::Vowel('ö'),
    Step::Vowel('i'),
    Step::Vowel('í'),
    Step::Vowel('ő'),
    Step::Vowel('ú'),
    Step::Double('l'),
    Step::Double('b'),
    Step::Single('t'),
    Step::Single('l'),
];

fn unpack(data: &[u8]) -> Option<String> {
    let mut text = String::new();
    flate2::read::GzDecoder::new(data).read_to_string(&mut text).ok()?;
    Some(text)
}

/// Replaces flag aliases ("ablak/12" means the 12th AF line) by the flags.
fn expand(word: &str, aliases: &[&str]) -> String {
    if let Some((stem, n)) = word.rsplit_once('/') {
        if let Some(flags) = n.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|i| aliases.get(i)) {
            return format!("{stem}/{flags}");
        }
    }
    word.to_string()
}

fn load() -> Option<spellbook::Dictionary> {
    let (aff, dic) = (unpack(AFF)?, unpack(DIC)?);
    let aliases: Vec<&str> = aff.lines().filter_map(|l| l.strip_prefix("AF ")).skip(1).collect();
    let mut affixes = String::with_capacity(aff.len() * 2);
    for line in aff.lines().filter(|l| !l.starts_with("AF ")) {
        let mut f: Vec<&str> = line.split(' ').collect();
        let expanded;
        if matches!(f[0], "SFX" | "PFX") && f.len() >= 4 && !(matches!(f[2], "Y" | "N") && f[3].parse::<u32>().is_ok()) {
            expanded = expand(f[3], &aliases);
            f[3] = &expanded;
            affixes.push_str(&f.join(" "));
        } else {
            affixes.push_str(line);
        }
        affixes.push('\n');
    }
    let mut words = String::with_capacity(dic.len() * 4);
    for line in dic.lines() {
        words.push_str(&expand(line, &aliases));
        words.push('\n');
    }
    spellbook::Dictionary::new(&affixes, &words).ok()
}

fn dictionary() -> Option<Arc<spellbook::Dictionary>> {
    let mut slot = DICTIONARY.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        *slot = load().map(Arc::new);
    }
    slot.clone()
}

/// Frees the dictionary; the next `fix` loads it again.
pub fn unload() {
    *DICTIONARY.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn other_vowel(c: char) -> Option<char> {
    Some(match c {
        'o' => 'ó',
        'ó' => 'o',
        'ö' => 'ő',
        'ő' => 'ö',
        'u' => 'ú',
        'ú' => 'u',
        'ü' => 'ű',
        'ű' => 'ü',
        'i' => 'í',
        'í' => 'i',
        _ => return None,
    })
}

/// Every spelling one step away, with the step that leads there.
fn candidates(word: &str) -> BTreeMap<String, Step> {
    let mut found = BTreeMap::new();
    for &(from, to) in SOUND_PAIRS {
        for (at, _) in word.match_indices(from) {
            found.insert(format!("{}{to}{}", &word[..at], &word[at + from.len()..]), Step::Pair(from, to));
        }
    }
    let letters: Vec<char> = word.chars().collect();
    let joined = |before: &[char], mid: &[char], after: &[char]| before.iter().chain(mid).chain(after).collect::<String>();
    for (i, &c) in letters.iter().enumerate() {
        if let Some(v) = other_vowel(c) {
            found.insert(joined(&letters[..i], &[v], &letters[i + 1..]), Step::Vowel(c));
        }
        if !VOWELS.contains(c) {
            found.insert(joined(&letters[..i], &[c, c], &letters[i + 1..]), Step::Double(c));
            if letters.get(i + 1) == Some(&c) {
                found.insert(joined(&letters[..i], &[], &letters[i + 1..]), Step::Single(c));
            }
        }
    }
    found.remove(word);
    found
}

fn repair(word: &str, dict: &spellbook::Dictionary) -> Option<String> {
    let lower = word.to_lowercase();
    if word.chars().count() < MIN_LETTERS
        || !word.chars().all(char::is_alphabetic)
        || word.contains(char::is_uppercase)
        || dict.check(word)
        || dict.check(&lower)
    {
        return None;
    }
    let mut known = candidates(&lower).into_iter().filter(|(w, _)| dict.check(w));
    let (fixed, step) = known.next()?;
    (known.next().is_none() && STEPS.contains(&step)).then_some(fixed)
}

/// Repairs the Hungarian words of `text`; `keep` says which words are the
/// user's own. Spaces, line breaks and punctuation stay as they are.
pub fn fix(text: &str, keep: impl Fn(&str) -> bool) -> String {
    let Some(dict) = dictionary() else { return text.to_string() };
    let mut out = String::with_capacity(text.len());
    for piece in text.split_inclusive(char::is_whitespace) {
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let core = piece.trim_matches(|c| !is_word(c));
        let handle = piece.starts_with(['@', '#', '/']);
        match (!core.is_empty() && !handle && !keep(core)).then(|| repair(core, &dict)).flatten() {
            Some(fixed) => out.push_str(&piece.replacen(core, &fixed, 1)),
            None => out.push_str(piece),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(text: &str) -> String {
        fix(text, |_| false)
    }

    #[test]
    fn the_dictionary_loads_and_knows_inflected_words() {
        let d = dictionary().expect("dictionary");
        for w in ["ablakok", "abbahagyni", "palástja", "méri", "információk", "megbeszélésére", "kutyáknak", "őszintén"] {
            assert!(d.check(w), "{w}");
        }
        for w in ["palástya", "méli", "informaciók"] {
            assert!(!d.check(w), "{w}");
        }
    }

    #[test]
    fn a_word_that_does_not_exist_becomes_its_one_sound_alike() {
        assert_eq!(f("A kórhadó fában él."), "A korhadó fában él.");
        assert_eq!(f("Sok munició maradt,"), "Sok muníció maradt,");
    }

    #[test]
    fn names_known_words_and_short_words_stay() {
        for t in ["Melkor sokáig uralkodott.", "Orlovában született.", "A gyóni földbirtok.", "Kúrá hó.", "A deadline-ra kész lesz.", "Elküldtem 3x a JSON-t.", ""] {
            assert_eq!(f(t), t);
        }
        assert_eq!(f("Melkor és\n  Orlova,  ott."), "Melkor és\n  Orlova,  ott.");
    }

    #[test]
    fn code_addresses_and_mixed_case_stay() {
        for t in ["municióId", "munició.ts", "@munició", "#munició", "munició_x", "https://munició.hu/munició", "munició-ban"] {
            assert_eq!(f(t), t);
        }
    }

    #[test]
    fn the_users_own_words_stay() {
        assert_eq!(fix("Sok munició maradt.", |w| w == "munició"), "Sok munició maradt.");
    }

    #[test]
    fn several_possible_words_mean_no_change() {
        let d = dictionary().unwrap();
        for w in ["méli", "xqzw", "abcd"] {
            assert_eq!(repair(w, &d), None, "{w}");
        }
    }
}
