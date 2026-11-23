//! Snippets: a short spoken trigger that types a longer saved text.
//!
//! A dictation fires a snippet only when the whole cleaned utterance is the
//! trigger, compared without case, accents, punctuation or extra spaces, so
//! "Címem." , "cimem" and "CÍMEM!" all fire the trigger "címem". The saved text
//! is inserted verbatim: no cleanup and no style pass touch it.

/// The comparison key: lower case, Latin accents removed, every run of
/// characters that are not letters or digits turned into one space, trimmed.
/// Empty when the text has no letter or digit.
pub fn match_key(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        let c = fold(c);
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    out
}

/// `key` (from [`match_key`]) with its spaces removed: the fallback
/// comparison that lets "e-mail", "e mail" and "email" meet.
pub fn joined_key(key: &str) -> String {
    key.chars().filter(|c| *c != ' ').collect()
}

/// Removes the accent of a lower-case Latin letter (Hungarian and the common
/// Western European ones); anything else is returned unchanged.
fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ő' | 'ø' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ç' => 'c',
        'ñ' => 'n',
        c => c,
    }
}

/// The saved text of the snippet whose trigger is the whole `utterance`, if
/// any. `snippets` holds `(trigger, text)` pairs; the first match wins.
pub fn find<'a>(utterance: &str, snippets: &'a [(String, String)]) -> Option<&'a str> {
    let key = match_key(utterance);
    if key.is_empty() {
        return None;
    }
    let exact = snippets.iter().find(|(trigger, _)| match_key(trigger) == key);
    let joined = joined_key(&key);
    exact
        .or_else(|| snippets.iter().find(|(trigger, _)| joined_key(&match_key(trigger)) == joined))
        .map(|(_, text)| text.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> Vec<(String, String)> {
        vec![
            ("címem".into(), "1051 Budapest, Példa utca 1.".into()),
            ("my email sign-off".into(), "Best,\nAlex".into()),
            ("Zoom link".into(), "https://example.com/j/123".into()),
        ]
    }

    #[test]
    fn key_ignores_case_accents_punctuation_and_spaces() {
        assert_eq!(match_key("  Címem! "), "cimem");
        assert_eq!(match_key("Árvíztűrő tükörfúrógép."), "arvizturo tukorfurogep");
        assert_eq!(match_key("my e-mail, sign-off"), "my e mail sign off");
        assert_eq!(match_key("...?!"), "");
        assert_eq!(match_key("Zoom-link"), match_key("zoom link"));
    }

    #[test]
    fn whole_utterance_fires_the_snippet_verbatim() {
        let s = set();
        assert_eq!(find("Címem.", &s), Some("1051 Budapest, Példa utca 1."));
        assert_eq!(find("cimem", &s), Some("1051 Budapest, Példa utca 1."));
        assert_eq!(find("My email sign off.", &s), Some("Best,\nAlex"));
        assert_eq!(find("Zoom-link!", &s), Some("https://example.com/j/123"));
    }

    #[test]
    fn hyphenated_or_split_spellings_still_fire() {
        let s = vec![("my email".to_string(), "me@example.com".to_string())];
        assert_eq!(find("My e-mail.", &s), Some("me@example.com"));
        assert_eq!(find("my e mail", &s), Some("me@example.com"));
        assert_eq!(find("My email!", &s), Some("me@example.com"));
        let s = vec![("my e-mail".to_string(), "me@example.com".to_string())];
        assert_eq!(find("My email.", &s), Some("me@example.com"));
        // An exact match wins over a joined one.
        let s = vec![("e mail".to_string(), "a".to_string()), ("email".to_string(), "b".to_string())];
        assert_eq!(find("Email.", &s), Some("b"));
        assert_eq!(find("E-mail.", &s), Some("a"));
        // Still the whole utterance only.
        assert_eq!(find("Send my e-mail.", &[("my email".into(), "x".into())]), None);
    }

    #[test]
    fn a_trigger_inside_a_longer_dictation_does_not_fire() {
        let s = set();
        assert_eq!(find("Elküldöm a címem holnap.", &s), None);
        assert_eq!(find("Címem és a telefonszámom.", &s), None);
        assert_eq!(find("", &s), None);
        assert_eq!(find("…", &s), None);
        // An empty trigger never matches an empty utterance.
        assert_eq!(find(".", &[(".".into(), "x".into())]), None);
    }
}
