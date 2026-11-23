//! Learning from edits: after a dictation is inserted, the app
//! reads the field again a few seconds later. If the user fixed a word that came
//! from the dictation, `learned_correction` returns the pair so the app can offer
//! "Add to dictionary?". Pure text comparison: nothing is stored here.

/// Longest correction offered, per side, in words and characters. Bigger edits
/// are rewrites, not a misheard word.
const MAX_WORDS: usize = 3;
const MAX_CHARS: usize = 40;

/// A word the user corrected in dictated text: `wrong` is what was typed by
/// the dictation, `right` what the user changed it to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub wrong: String,
    pub right: String,
}

/// Compares the field right after an insertion (`before`) with the same field
/// later (`after`). Returns a correction when exactly one stretch of up to three
/// words inside the dictated text `dictated` was replaced by other words.
///
/// Returns `None` for typing that only adds or removes whole words (the user
/// kept writing, or dropped a filler), for punctuation-only changes, for edits
/// outside the dictated text, and for rewrites longer than three words.
pub fn learned_correction(dictated: &str, before: &str, after: &str) -> Option<Correction> {
    let dictated = dictated.trim();
    if dictated.is_empty() || before == after {
        return None;
    }
    let a: Vec<char> = before.chars().collect();
    let b: Vec<char> = after.chars().collect();

    // Where the dictated text sits in the field (last occurrence: the insertion
    // point is usually the end of what the user has written so far).
    let byte_start = before.rfind(dictated)?;
    let d_start = before[..byte_start].chars().count();
    let d_end = d_start + dictated.chars().count();

    // The one changed stretch: everything between the common prefix and suffix.
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let max_suffix = a.len().min(b.len()) - prefix;
    let suffix = a.iter().rev().zip(b.iter().rev()).take(max_suffix).take_while(|(x, y)| x == y).count();

    // Widen to whole words. The prefix is shared, so `start` is the same in both;
    // the suffix is shared, so `end` moves by the same amount in both.
    let mut start = prefix;
    while start > 0 && !a[start - 1].is_whitespace() {
        start -= 1;
    }
    let mut end_a = a.len() - suffix;
    while end_a < a.len() && !a[end_a].is_whitespace() {
        end_a += 1;
    }
    let end_b = end_a + b.len() - a.len();

    if start < d_start || end_a > d_end {
        return None;
    }
    let old: String = a[start..end_a].iter().collect();
    let new: String = b[start..end_b].iter().collect();
    let wrong = trim_word(&old);
    let right = trim_word(&new);
    if wrong.is_empty() || right.is_empty() || wrong == right {
        return None;
    }
    if !right.chars().any(char::is_alphabetic) {
        return None;
    }
    let (wrong_words, right_words) = (words(wrong), words(right));
    if wrong_words.len() > MAX_WORDS
        || right_words.len() > MAX_WORDS
        || wrong.chars().count() > MAX_CHARS
        || right.chars().count() > MAX_CHARS
    {
        return None;
    }
    // Whole words added or removed around kept ones: the user wrote on, or
    // deleted a word, rather than fixing one.
    if is_word_run_of(&wrong_words, &right_words) || is_word_run_of(&right_words, &wrong_words) {
        return None;
    }
    Some(Correction { wrong: wrong.to_string(), right: right.to_string() })
}

/// Strips spaces and the punctuation around a word ("klózisz," → "klózisz").
fn trim_word(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || (!c.is_alphanumeric() && c != '-' && c != '\''))
        .trim_matches(|c: char| c == '-' || c == '\'')
}

fn words(s: &str) -> Vec<&str> {
    s.split_whitespace().map(trim_word).filter(|w| !w.is_empty()).collect()
}

/// True when `part` is a non-empty run of consecutive words in `whole`, and
/// `whole` is longer.
fn is_word_run_of(part: &[&str], whole: &[&str]) -> bool {
    !part.is_empty() && whole.len() > part.len() && whole.windows(part.len()).any(|w| w == part)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(dictated: &str, before: &str, after: &str) -> Option<(String, String)> {
        learned_correction(dictated, before, after).map(|c| (c.wrong, c.right))
    }

    fn some(wrong: &str, right: &str) -> Option<(String, String)> {
        Some((wrong.to_string(), right.to_string()))
    }

    #[test]
    fn one_misheard_word_is_learned() {
        let d = "Holnap a klózisz csapattal találkozom.";
        let after = "Holnap a Clausis csapattal találkozom.";
        assert_eq!(pair(d, d, after), some("klózisz", "Clausis"));
    }

    #[test]
    fn punctuation_next_to_the_word_is_left_out() {
        let d = "Szia klózisz, mi újság?";
        assert_eq!(pair(d, d, "Szia Clausis, mi újság?"), some("klózisz", "Clausis"));
        assert_eq!(pair(d, d, "Szia klózisz. mi újság?"), None);
    }

    #[test]
    fn dictation_after_earlier_text_in_the_field() {
        let d = "I met Shivon today.";
        let before = format!("Notes from Monday. {d}");
        let after = "Notes from Monday. I met Siobhan today.";
        assert_eq!(pair(d, &before, after), some("Shivon", "Siobhan"));
    }

    #[test]
    fn two_words_for_one_and_case_fixes_count() {
        let d = "push mogo is fast";
        assert_eq!(pair(d, d, "Pusmogo is fast"), some("push mogo", "Pusmogo"));
        assert_eq!(pair(d, d, "push Mogo is fast"), some("mogo", "Mogo"));
    }

    #[test]
    fn writing_on_or_deleting_words_is_not_a_correction() {
        let d = "Holnap találkozunk";
        assert_eq!(pair(d, d, "Holnap találkozunk és eszünk"), None);
        assert_eq!(pair(d, d, "Holnap találkozunk."), None);
        assert_eq!(pair(d, d, "Holnap"), None);
        assert_eq!(pair("hát Holnap jövök", "hát Holnap jövök", "Holnap jövök"), None);
        assert_eq!(pair(d, d, "Holnap nagyon találkozunk"), None);
    }

    #[test]
    fn edits_outside_the_dictation_are_ignored() {
        let d = "a klózisz csapat";
        let before = format!("Elso sor. {d}");
        assert_eq!(pair(d, &before, "Első sor. a klózisz csapat"), None);
        assert_eq!(pair(d, &before, &format!("{before} itt")), None);
    }

    #[test]
    fn rewrites_and_unrelated_fields_are_ignored() {
        let d = "egy kettő három négy öt";
        assert_eq!(pair(d, d, "hat hét nyolc kilenc tíz"), None);
        assert_eq!(pair(d, "something else", "something new"), None);
        assert_eq!(pair(d, d, d), None);
        assert_eq!(pair("", "x", "y"), None);
        assert_eq!(pair("ár 15 forint", "ár 15 forint", "ár 16 forint"), None);
    }

    #[test]
    fn unicode_offsets_stay_aligned() {
        let d = "Ő az ügyvéd, Kovács Ödön";
        assert_eq!(pair(d, d, "Ő az ügyvéd, Kovách Ödön"), some("Kovács", "Kovách"));
        assert_eq!(pair(d, &format!("🙂 {d}"), "🙂 Ő az ügyvéd, Kovács Ödönke"), some("Ödön", "Ödönke"));
    }
}
