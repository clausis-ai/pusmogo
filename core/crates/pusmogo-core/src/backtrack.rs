//! Spoken self-corrections ("backtrack"): "Holnap háromkor, nem, négykor
//! találkozunk." → "Holnap négykor találkozunk."
//!
//! Runs on text that `clean()` already produced. It only drops words, never
//! adds or changes one, and only in a narrow shape:
//!
//! `<wrong>, <cue>[,] [mégis|inkább] <right>`
//!
//! where `<cue>` is a correction phrase ("nem", "bocsánat", "vagyis inkább",
//! "akarom mondani", "no", "sorry", "I mean", ...) and `<wrong>` and `<right>`
//! are the same kind of word: both numbers, both days or times, or both names
//! (capitalised mid-sentence). "Ez nem kettő, hanem három." and "A határidő
//! péntek, vagyis két napunk maradt." keep every word: "hanem" is no cue, and
//! a bare "vagyis" means "that is", not a correction.
//!
//! A word repeated in front of both halves is dropped with the wrong half:
//! "at 3, no, at 4" → "at 4".
//!
//! The app runs it after cleanup on every dictation unless the user turns
//! "Catch self-corrections" off in Settings (`pusmogo_set_self_corrections`).

use crate::lang;

/// One-word cues: must carry their own comma ("nem," not "nem").
const CUES_ONE: &[&str] = &[
    "nem", "bocsánat", "bocs", "pardon", "elnézést", "javítom", "helyesebben", "pontosabban",
    "no", "sorry", "rather",
];

/// Two-word cues: the comma after the second word is optional.
const CUES_TWO: &[(&str, &str)] = &[
    ("vagyis", "inkább"),
    ("illetve", "inkább"),
    ("akarom", "mondani"),
    ("i", "mean"),
    ("or", "rather"),
];

/// Words that may follow a cue and go with it ("nem, mégis Katának").
const SOFTENERS: &[&str] = &["mégis", "inkább", "actually"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Number,
    When,
    Name,
}

/// Word without surrounding punctuation, lower-cased.
fn core(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != ':')
        .trim_end_matches(['-', ':'])
        .to_lowercase()
}

fn is_hu_number(word: &str) -> bool {
    lang::list(lang::NUMBER_ENDINGS).iter().any(|suffix| {
        word.strip_suffix(suffix)
            .is_some_and(|stem| !stem.is_empty() && segments_into_numbers(stem))
    })
}

fn segments_into_numbers(stem: &str) -> bool {
    stem.is_empty()
        || lang::list(lang::NUMBER_PARTS)
            .iter()
            .any(|p| stem.strip_prefix(p).is_some_and(segments_into_numbers))
}

/// `at_start`: the token opens a sentence, so a capital letter says nothing.
fn kind(token: &str, at_start: bool) -> Option<Kind> {
    let word = core(token);
    let first = word.chars().next()?;
    if first.is_ascii_digit() || is_hu_number(&word) || lang::contains(lang::NUMBER_WORDS, &word) {
        return Some(Kind::Number);
    }
    if lang::contains(lang::WHEN_WORDS, &word) || lang::list(lang::WHEN_STEMS).iter().any(|s| word.starts_with(s.as_str())) {
        return Some(Kind::When);
    }
    let shown = token.trim_start_matches(|c: char| !c.is_alphanumeric());
    if !at_start && shown.chars().next().is_some_and(char::is_uppercase) && word != "i" {
        return Some(Kind::Name);
    }
    None
}

fn ends_sentence(token: &str) -> bool {
    token.ends_with(['.', '?', '!', ':']) || token.ends_with('…')
}

/// Ends in exactly one comma ("háromkor," but not "háromkor" or "x.,").
fn ends_in_comma(token: &str) -> bool {
    token.ends_with(',') && !token[..token.len() - 1].ends_with(|c: char| !c.is_alphanumeric())
}

/// Length of the cue starting at `i` (with any softener), or 0.
fn cue_len(tokens: &[&str], i: usize) -> usize {
    let word = |k: usize| tokens.get(k).map(|t| core(t)).unwrap_or_default();
    let mut used = 0;
    if CUES_ONE.contains(&word(i).as_str()) && ends_in_comma(tokens[i]) {
        used = 1;
    } else if CUES_TWO.iter().any(|(a, b)| word(i) == *a && word(i + 1) == *b) {
        let bare = |k: usize| tokens[k].chars().last().is_some_and(char::is_alphanumeric);
        if bare(i) && (bare(i + 1) || ends_in_comma(tokens[i + 1])) {
            used = 2;
        }
    }
    if used > 0 && SOFTENERS.contains(&word(i + used).as_str()) && i + used + 1 < tokens.len() {
        used += 1;
    }
    used
}

/// Drops "<wrong>, <cue>," before a correction of the same kind; see the module docs.
pub fn drop_self_corrections(text: &str) -> String {
    text.split('\n').map(drop_in_line).collect::<Vec<_>>().join("\n")
}

fn drop_in_line(line: &str) -> String {
    let mut tokens: Vec<String> = line.split(' ').map(str::to_string).collect();
    let mut j = 0;
    while j + 2 < tokens.len() {
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        if let Some((from, to)) = correction_at(&refs, j) {
            let opens = from == 0 || ends_sentence(refs[from - 1]);
            if opens {
                tokens[to] = capitalise(&tokens[to]);
            }
            tokens.drain(from..to);
            j = from;
            continue;
        }
        j += 1;
    }
    tokens.join(" ")
}

/// For a wrong half ending at `j`: the token range to drop (end exclusive).
fn correction_at(tokens: &[&str], j: usize) -> Option<(usize, usize)> {
    if !ends_in_comma(tokens[j]) {
        return None;
    }
    let used = cue_len(tokens, j + 1);
    if used == 0 {
        return None;
    }
    let to = j + 1 + used;
    let mut right = to;
    let at_start = |k: usize| k == 0 || ends_sentence(tokens[k - 1]);
    let wrong = kind(tokens[j], at_start(j))?;
    let mut from = j;
    // "at 3, no, at 4": the word before the wrong half comes back before the right one.
    if j > 0
        && right + 1 < tokens.len()
        && core(tokens[right]) == core(tokens[j - 1])
        && !tokens[j - 1].ends_with(|c: char| !c.is_alphanumeric())
    {
        from = j - 1;
        right += 1;
    }
    let right_token = tokens.get(right)?;
    (kind(right_token, false) == Some(wrong)).then_some((from, to))
}

fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::drop_self_corrections as drop;

    #[test]
    fn hungarian_corrections_keep_only_the_right_half() {
        let cases = [
            ("Holnap háromkor, nem, négykor találkozunk.", "Holnap négykor találkozunk."),
            ("Küldd el Péternek, vagyis inkább Annának a fájlt.", "Küldd el Annának a fájlt."),
            ("A megbeszélés kedden, bocsánat, szerdán lesz.", "A megbeszélés szerdán lesz."),
            ("Vegyél két, nem, három kiló krumplit.", "Vegyél három kiló krumplit."),
            ("10, nem, 20 percet késem.", "20 percet késem."),
            ("Az új verzió pénteken, akarom mondani, csütörtökön jön ki.", "Az új verzió csütörtökön jön ki."),
            ("A számla végösszege 80, javítom, 90 ezer forint.", "A számla végösszege 90 ezer forint."),
            ("Szólj Gábornak, nem, mégis Katának, hogy kész a jelentés.", "Szólj Katának, hogy kész a jelentés."),
            ("Reggel hétkor, illetve inkább nyolckor indulunk.", "Reggel nyolckor indulunk."),
            ("Hármat, nem, négyet kérek.", "Négyet kérek."),
            ("Kedden, nem, holnap jövök.", "Holnap jövök."),
        ];
        for (input, want) in cases {
            assert_eq!(drop(input), want, "{input}");
        }
    }

    #[test]
    fn english_corrections() {
        assert_eq!(drop("Let's meet at 3, no, at 4."), "Let's meet at 4.");
        assert_eq!(drop("Send it to Anna, sorry, Peter today."), "Send it to Peter today.");
        assert_eq!(drop("See you Monday, I mean Tuesday."), "See you Tuesday.");
    }

    #[test]
    fn ordinary_uses_of_cue_words_stay() {
        let keep = [
            "Nem, nem megyek el holnap.",
            "Ez nem kettő, hanem három.",
            "Nem a pénteki, hanem a szombati vonattal jövök.",
            "Hozz kenyeret, illetve ha van, egy kis sajtot is.",
            "A határidő péntek, vagyis két napunk maradt.",
            "Szerinted ez jó ötlet vagy nem?",
            "Bocsánat, hogy csak most válaszolok.",
            "Kovács Anna 2025. március 3-án 14:30-kor érkezik.",
            // Different kinds on the two sides.
            "Ezt a Slacken, bocs, e-mailben küldd át.",
            "Szia Anna, bocsánat, hogy késtem.",
            "Kérdezted, hogy jövök-e? Igen, nem, talán.",
            // A sentence-initial capital is no name.
            "Igen, nem, Péter jön.",
            // The cue needs its comma.
            "Holnap háromkor, nem négykor.",
            "No, I mean it.",
            "I asked two, no, nobody answered.",
            // Nothing after the right half's cue.
            "Három, nem,",
        ];
        for text in keep {
            assert_eq!(drop(text), text, "{text}");
        }
    }

    #[test]
    fn line_breaks_survive() {
        assert_eq!(
            drop("Lista:\nkettő, nem, három alma\nés körte"),
            "Lista:\nHárom alma\nés körte"
        );
    }

    #[test]
    fn number_words_need_whole_parts() {
        // "hatalmas" starts with "hat" but is no number.
        assert_eq!(drop("Ez hatalmas, nem, kicsi."), "Ez hatalmas, nem, kicsi.");
        assert_eq!(drop("Tizenkettő, nem, huszonöt fő jön."), "Huszonöt fő jön.");
    }
}
