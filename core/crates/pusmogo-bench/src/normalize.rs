//! Text normaliser used on both the reference and the engine output before scoring,
//! so "huszonhét" and "27", "40%" and "forty percent", "3-kor" and "háromkor" compare equal.
//!
//! Steps: lowercase → "%" to a word → join hyphenated words → drop punctuation →
//! number words to digits (Hungarian one-word numbers with a case suffix, English
//! multi-word numbers). Ambiguous words ("egy" = a/one, "hat", "hét") are mapped the
//! same way on both sides, so they cannot create errors, only hide none.

use pusmogo_core::Language;

/// Hungarian number morphemes, longest first so "hatvan" wins over "hat".
const HU_MORPHEMES: &[(&str, u64)] = &[
    ("kilencven", 90), ("nyolcvan", 80), ("harminc", 30), ("negyven", 40), ("hetven", 70),
    ("hatvan", 60), ("ötven", 50), ("huszon", 20), ("millió", 1_000_000), ("kilenc", 9),
    ("kettő", 2), ("három", 3), ("tizen", 10), ("nyolc", 8), ("négy", 4), ("száz", 100),
    ("ezer", 1000), ("húsz", 20), ("egy", 1), ("két", 2), ("hat", 6), ("hét", 7), ("tíz", 10),
    ("öt", 5), ("nulla", 0),
];

/// Case and derivational endings that may follow a Hungarian number word or digit
/// ("háromkor", "3-kor", "péntekig"-style). A token is only converted when the rest
/// after the number is empty or exactly one of these, so "hatalmas" stays a word.
const HU_SUFFIXES: &[&str] = &[
    "kor", "ig", "ra", "re", "ról", "ről", "ba", "be", "ban", "ben", "ból", "ből", "nál", "nél",
    "tól", "től", "hoz", "hez", "höz", "val", "vel", "szor", "szer", "ször", "at", "et", "öt",
    "t", "an", "en", "on", "ön", "as", "es", "os", "ös", "nak", "nek", "ért", "kal", "kel",
    "ból", "edik", "adik", "odik", "ödik", "ik",
];

/// "-val/-vel" (with) assimilates to the last consonant of the word it follows:
/// "hárommal", "öttel", "százalékkal". Writers (and Whisper) also produce the
/// wrong "15%-al". All of these are mapped to "val"/"vel" so the digit and word
/// forms compare equal; the suffix spelling itself is a cleanup concern, not
/// a recognition one.
const HU_WITH_SUFFIXES: &[&str] = &[
    "al", "el", "val", "vel", "kal", "kel", "tal", "tel", "mal", "mel", "cal", "cel", "zal",
    "zel", "nal", "nel", "nyal", "nyel", "dal", "del", "sal", "sel", "ral", "rel", "lal", "lel",
    "gal", "gel",
];

/// Canonical form of a "-val/-vel" suffix, or None if `rest` is not one.
fn canonical_with(rest: &str) -> Option<&'static str> {
    if !HU_WITH_SUFFIXES.contains(&rest) {
        return None;
    }
    Some(if rest.ends_with("al") { "val" } else { "vel" })
}

const EN_SMALL: &[(&str, u64)] = &[
    ("zero", 0), ("one", 1), ("two", 2), ("three", 3), ("four", 4), ("five", 5), ("six", 6),
    ("seven", 7), ("eight", 8), ("nine", 9), ("ten", 10), ("eleven", 11), ("twelve", 12),
    ("thirteen", 13), ("fourteen", 14), ("fifteen", 15), ("sixteen", 16), ("seventeen", 17),
    ("eighteen", 18), ("nineteen", 19), ("twenty", 20), ("thirty", 30), ("forty", 40),
    ("fifty", 50), ("sixty", 60), ("seventy", 70), ("eighty", 80), ("ninety", 90),
    ("hundred", 100), ("thousand", 1000), ("million", 1_000_000),
];

/// English ordinal words. Compounds ("twenty-first" → "twentyfirst") are a tens
/// word from `EN_SMALL` followed by one of the units here.
const EN_ORDINALS: &[(&str, u64)] = &[
    ("first", 1), ("second", 2), ("third", 3), ("fourth", 4), ("fifth", 5), ("sixth", 6),
    ("seventh", 7), ("eighth", 8), ("ninth", 9), ("tenth", 10), ("eleventh", 11),
    ("twelfth", 12), ("thirteenth", 13), ("fourteenth", 14), ("fifteenth", 15),
    ("sixteenth", 16), ("seventeenth", 17), ("eighteenth", 18), ("nineteenth", 19),
    ("twentieth", 20), ("thirtieth", 30), ("fortieth", 40), ("fiftieth", 50),
    ("sixtieth", 60), ("seventieth", 70), ("eightieth", 80), ("ninetieth", 90),
    ("hundredth", 100), ("thousandth", 1000),
];

/// British spellings whose American form differs by more than "-ise"/"-isation";
/// stems, matched with an optional plural or past ending.
const EN_BRITISH: &[(&str, &str)] = &[
    ("colour", "color"), ("favour", "favor"), ("behaviour", "behavior"), ("honour", "honor"),
    ("labour", "labor"), ("neighbour", "neighbor"), ("harbour", "harbor"), ("humour", "humor"),
    ("flavour", "flavor"), ("rumour", "rumor"), ("centre", "center"), ("metre", "meter"),
    ("theatre", "theater"), ("litre", "liter"), ("travelled", "traveled"),
    ("travelling", "traveling"), ("cancelled", "canceled"), ("defence", "defense"),
    ("offence", "offense"), ("licence", "license"), ("catalogue", "catalog"),
    ("programme", "program"), ("grey", "gray"), ("aluminium", "aluminum"),
];

/// "eleventh" → "11th", "twentyfirst" → "21st", "11th" stays; None if not an ordinal.
fn en_ordinal(w: &str) -> Option<String> {
    let suffix = |n: u64| match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    let find = |w: &str| EN_ORDINALS.iter().find(|(k, _)| *k == w).map(|(_, v)| *v);
    let n = find(w).or_else(|| {
        EN_SMALL.iter().find_map(|(tens, t)| {
            let unit = find(w.strip_prefix(tens)?)?;
            (*t >= 20 && *t < 100 && unit < 10).then_some(t + unit)
        })
    })?;
    Some(format!("{n}{}", suffix(n)))
}

/// American spelling for scoring: "organised" → "organized", "colours" → "colors".
/// Applied to both sides, so a wrong fold of a word that is not British
/// ("exercise" → "exercize") cannot create an error.
fn en_spelling(w: String) -> String {
    for (gb, us) in EN_BRITISH {
        if let Some(rest) = w.strip_prefix(gb) {
            if ["", "s", "ed", "ing", "d"].contains(&rest) {
                return format!("{us}{rest}");
            }
        }
    }
    // -ise/-ised/-ises/-ising/-isation(s) on words of 6+ letters ("rise", "wise" stay).
    for (gb, us) in [
        ("isations", "izations"), ("isation", "ization"), ("ising", "izing"), ("ised", "ized"),
        ("ises", "izes"), ("ise", "ize"),
    ] {
        if let Some(stem) = w.strip_suffix(gb) {
            if stem.chars().count() >= 3 {
                return format!("{stem}{us}");
            }
        }
    }
    w
}

/// Normalises text for scoring. Returns the words joined by single spaces.
pub fn normalize(text: &str, lang: Language) -> String {
    let percent = match lang {
        Language::English => " percent ",
        _ => " százalék",
    };
    let lower = text.to_lowercase().replace('%', percent);

    // Keep letters, digits and hyphens; everything else separates words.
    let mut cleaned = String::with_capacity(lower.len());
    for ch in lower.chars() {
        if ch.is_alphanumeric() || ch == '-' {
            cleaned.push(ch);
        } else if ch == '\'' || ch == '’' {
            // "let's" → "lets": apostrophes join, they never split.
        } else {
            cleaned.push(' ');
        }
    }

    let mut words: Vec<String> = Vec::new();
    for raw in cleaned.split_whitespace() {
        // "kétezer-háromszáz", "3-kor", "twenty-seven" → one word; lone "-" disappears.
        let w: String = raw.chars().filter(|c| *c != '-').collect();
        if w.is_empty() {
            continue;
        }
        let w = if lang == Language::English {
            en_ordinal(&w).unwrap_or_else(|| en_spelling(w))
        } else {
            hu_word(w)
        };
        words.push(w);
    }

    if lang != Language::Hungarian {
        words = en_numbers(&words);
    }
    words.join(" ")
}

/// Hungarian per-word canonical form: number words to digits, and the
/// "százalék" + "-val/-vel" suffix ("százalékkal", "%-al") to one spelling.
fn hu_word(w: String) -> String {
    if let Some(rest) = w.strip_prefix("százalék") {
        if let Some(c) = canonical_with(rest) {
            return format!("százalék{c}");
        }
    }
    hu_number_word(&w).unwrap_or(w)
}

/// "huszonhét" → "27", "háromkor" → "3kor", "3kor" stays, "hatalmas" → None.
fn hu_number_word(word: &str) -> Option<String> {
    // Digits followed by an allowed suffix are already canonical.
    let digits_end = word.find(|c: char| !c.is_ascii_digit()).unwrap_or(word.len());
    if digits_end > 0 {
        let (digits, rest) = word.split_at(digits_end);
        return canonical_with(rest).map(|c| format!("{digits}{c}"));
    }

    let mut rest = word;
    let mut values = Vec::new();
    'outer: while !rest.is_empty() {
        for (m, v) in HU_MORPHEMES {
            if let Some(r) = rest.strip_prefix(m) {
                values.push(*v);
                rest = r;
                continue 'outer;
            }
        }
        break;
    }
    if values.is_empty()
        || !(rest.is_empty() || HU_SUFFIXES.contains(&rest) || canonical_with(rest).is_some())
    {
        return None;
    }
    // A bare suffix like "hatt" or "egyet" is fine; but "egyben"/"hatan" are real
    // words too, and both sides get the same treatment, so that is harmless.
    let n = compose(&values)?;
    Some(format!("{n}{}", canonical_with(rest).unwrap_or(rest)))
}

/// Builds a number from morpheme values: 2,1000,3,100,20,7 → 2327.
/// Returns None for sequences that are not a valid number ("hathat").
fn compose(values: &[u64]) -> Option<u64> {
    let mut total = 0u64;
    let mut current = 0u64;
    let mut last_small: Option<u64> = None;
    for &v in values {
        match v {
            100 => {
                current = current.max(1) * 100;
                last_small = None;
            }
            1000 | 1_000_000 => {
                total += current.max(1) * v;
                current = 0;
                last_small = None;
            }
            _ => {
                // Two units in a row ("hathat") is not a number.
                if let Some(prev) = last_small {
                    if prev < 10 || v >= prev {
                        return None;
                    }
                }
                current += v;
                last_small = Some(v);
            }
        }
    }
    Some(total + current)
}

/// Joins runs of English number words: "twenty seven" → "27",
/// "two hundred and five" → "205". A lone "and" outside a number is kept.
fn en_numbers(words: &[String]) -> Vec<String> {
    let value = |w: &str| EN_SMALL.iter().find(|(k, _)| *k == w).map(|(_, v)| *v);
    // "twentyseven" (from "twenty-seven") splits into known parts.
    let split = |w: &str| -> Option<Vec<u64>> {
        if let Some(v) = value(w) {
            return Some(vec![v]);
        }
        for (k, v) in EN_SMALL {
            if let Some(r) = w.strip_prefix(k) {
                if let Some(v2) = value(r) {
                    if *v >= 20 && v2 < 10 {
                        return Some(vec![*v, v2]);
                    }
                }
            }
        }
        None
    };

    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut j = i;
        while j < words.len() {
            let joins_and = words[j] == "and" && j > i && j + 1 < words.len() && split(&words[j + 1]).is_some();
            if split(&words[j]).is_some() || joins_and {
                j += 1;
            } else {
                break;
            }
        }
        // Longest run that is one valid number; "one two three" falls back to singles.
        let mut done = false;
        let mut end = j;
        while end > i {
            let part: Vec<u64> = words[i..end]
                .iter()
                .filter(|w| *w != "and")
                .flat_map(|w| split(w).unwrap_or_default())
                .collect();
            if let Some(n) = compose_en(&part) {
                out.push(n.to_string());
                i = end;
                done = true;
                break;
            }
            end -= 1;
        }
        if !done {
            out.push(words[i].clone());
            i += 1;
        }
    }
    out
}

fn compose_en(values: &[u64]) -> Option<u64> {
    let mut total = 0u64;
    let mut current = 0u64;
    let mut last: Option<u64> = None;
    for &v in values {
        match v {
            100 => current = current.max(1) * 100,
            1000 | 1_000_000 => {
                total += current.max(1) * v;
                current = 0;
            }
            _ => {
                if let Some(p) = last {
                    // "one two" is a digit sequence, not a number; only tens + unit chain.
                    if p != 100 && p < 1000 && !(p >= 20 && p % 10 == 0 && v < 10) {
                        return None;
                    }
                }
                current += v;
            }
        }
        last = Some(v);
    }
    Some(total + current)
}

/// Folds Hungarian (and common Latin) accents: "őrült" → "orult". For the
/// accent-insensitive CER, which shows how much of the error is only diacritics.
pub fn fold_accents(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'ő' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'ű' => 'u',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hu(s: &str) -> String {
        normalize(s, Language::Hungarian)
    }
    fn en(s: &str) -> String {
        normalize(s, Language::English)
    }

    #[test]
    fn lowercases_and_strips_punctuation() {
        assert_eq!(hu("Kérlek, küldd el a jelentést péntekig!"), "kérlek küldd el a jelentést péntekig");
        assert_eq!(en("Let's go, now."), "lets go now");
    }

    #[test]
    fn hungarian_number_words_become_digits() {
        assert_eq!(hu("Összesen huszonhét ügyfél"), "összesen 27 ügyfél");
        assert_eq!(hu("Összesen 27 ügyfél"), "összesen 27 ügyfél");
        assert_eq!(hu("kétezer-háromszázhuszonhét"), "2327");
        assert_eq!(hu("ezerkilencszázötvenhat"), "1956");
        assert_eq!(hu("tizenkettő"), "12");
        assert_eq!(hu("hatvan"), "60");
        assert_eq!(hu("száz"), "100");
    }

    #[test]
    fn hungarian_number_with_suffix_matches_digit_form() {
        assert_eq!(hu("háromkor"), "3kor");
        assert_eq!(hu("3-kor"), "3kor");
        assert_eq!(hu("ötször"), "5ször");
    }

    #[test]
    fn hungarian_with_suffix_after_numbers_and_percent() {
        // synth-01 hu014/hu054: Whisper writes "15%-al" for "tizenöt százalékkal".
        assert_eq!(hu("tizenöt százalékkal"), hu("15%-al"));
        assert_eq!(hu("15%-kal"), hu("15%-al"));
        assert_eq!(hu("harminc százalékkal"), "30 százalékval");
        assert_eq!(hu("hárommal"), hu("3-mal"));
        assert_eq!(hu("öttel"), hu("5-tel"));
        // synth-01 hu055: already equal, keep it that way.
        assert_eq!(hu("nyolckor"), hu("8-kor"));
    }

    #[test]
    fn hungarian_words_that_start_like_numbers_stay() {
        assert_eq!(hu("hatalmas hétfő egyetem"), "hatalmas hétfő egyetem");
        assert_eq!(hu("ötlet négyzet"), "ötlet négyzet");
    }

    #[test]
    fn percent_both_ways() {
        assert_eq!(hu("40%-kal nőtt"), hu("negyven százalékkal nőtt"));
        assert_eq!(en("dropped by 40%"), en("dropped by forty percent"));
        assert_eq!(en("dropped by 40%"), "dropped by 40 percent");
    }

    #[test]
    fn english_number_words() {
        assert_eq!(en("twenty-seven clients"), "27 clients");
        assert_eq!(en("twenty seven clients"), "27 clients");
        assert_eq!(en("two hundred and five"), "205");
        assert_eq!(en("rock and roll"), "rock and roll");
        assert_eq!(en("one two three"), "1 2 3");
    }

    #[test]
    fn english_ordinals_match_digit_forms() {
        // fleurs-en-01 finding 2.
        assert_eq!(en("finished eleventh"), en("finished 11th"));
        assert_eq!(en("sixteenth"), "16th");
        assert_eq!(en("twenty-first century"), "21st century");
        assert_eq!(en("the 22nd and the third"), "the 22nd and the 3rd");
        assert_eq!(en("twelfth night"), "12th night");
    }

    #[test]
    fn english_british_spellings_fold() {
        assert_eq!(en("organised by the Mau"), en("organized by the Mau"));
        assert_eq!(en("hospitalised"), "hospitalized");
        assert_eq!(en("the colours of the centre"), "the colors of the center");
        assert_eq!(en("organisation"), "organization");
        // Short words and "four"/"hour"/"your" are left alone.
        assert_eq!(en("rise wise four hour your"), "rise wise 4 hour your");
    }

    #[test]
    fn folds_accents() {
        assert_eq!(fold_accents("őrült ütő árvíztűrő tükörfúrógép"), "orult uto arvizturo tukorfurogep");
    }
}
