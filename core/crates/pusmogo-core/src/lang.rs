//! Per-language word lists for the rule-based cleanup, kept as small data
//! files (`lang/hu.toml`, `lang/en.toml`) compiled into the core.
//!
//! The cleanup runs the same rules whatever language was dictated (a Hungarian
//! dictation often carries English words), so a list lookup sees the union of
//! every pack. The files use a small subset of TOML: comments, `key = "value"`,
//! `key = ["a", "b"]` arrays that may span lines, and `[table]` headers that
//! make the keys after them `table.key`. Adding a language means
//! adding a file and a line to `PACKS`; see docs/adding-a-language.md.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Every compiled-in pack, as (language code, file contents).
const PACKS: &[(&str, &str)] = &[
    ("hu", include_str!("lang/hu.toml")),
    ("en", include_str!("lang/en.toml")),
];

/// Lists the cleanup reads. A pack may leave any of them out.
pub(crate) const FILLERS: &str = "fillers";
pub(crate) const STUTTER_WORDS: &str = "stutter_words";
pub(crate) const ABBREVIATIONS: &str = "abbreviations";
pub(crate) const LOWERCASE_ABBREVIATIONS: &str = "lowercase_abbreviations";
pub(crate) const NOUN_MARKERS: &str = "noun_markers";
pub(crate) const AMBIGUOUS_MARKS: &str = "ambiguous_marks";
/// Words whose full stop the casual styles keep at the end of a message.
pub(crate) const KEEP_FINAL_STOP: &str = "keep_final_stop";
/// Number and date words for spoken self-corrections (`backtrack.rs`).
/// Whole number words ("seven").
pub(crate) const NUMBER_WORDS: &str = "number_words";
/// Parts a compound number word is built from ("tizen" + "három").
pub(crate) const NUMBER_PARTS: &str = "number_parts";
/// Endings a number or day may carry ("hárm-at"); "" for none.
pub(crate) const NUMBER_ENDINGS: &str = "number_endings";
/// Day, month and time-of-day stems that take endings ("kedd-en").
pub(crate) const WHEN_STEMS: &str = "when_stems";
/// Day and time words matched whole ("ma", "today").
pub(crate) const WHEN_WORDS: &str = "when_words";

type Lists = HashMap<String, Vec<String>>;

/// Parses one pack. Returns the scalar `language` and every array by key.
fn parse(src: &str) -> Result<(String, Lists), String> {
    let mut language = String::new();
    let mut lists = Lists::new();
    let mut open: Option<(String, Vec<String>)> = None;
    let mut table = String::new();
    for (n, raw) in src.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if open.is_none() && line.starts_with('[') {
            let name = line[1..].strip_suffix(']').map(str::trim).unwrap_or_default();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!("line {}: expected [table]", n + 1));
            }
            table = format!("{name}.");
            continue;
        }
        let rest = if let Some((key, mut items)) = open.take() {
            let (done, rest) = read_items(line, &mut items).map_err(|e| format!("line {}: {e}", n + 1))?;
            if done {
                lists.entry(key).or_default().extend(items);
            } else {
                open = Some((key, items));
            }
            rest
        } else {
            let (key, value) = line.split_once('=').ok_or(format!("line {}: expected key = value", n + 1))?;
            let (key, value) = (format!("{table}{}", key.trim()), value.trim());
            if let Some(body) = value.strip_prefix('[') {
                let mut items = Vec::new();
                let (done, rest) = read_items(body, &mut items).map_err(|e| format!("line {}: {e}", n + 1))?;
                if done {
                    lists.entry(key).or_default().extend(items);
                } else {
                    open = Some((key, items));
                }
                rest
            } else {
                let s = unquote(value).ok_or(format!("line {}: expected a quoted string", n + 1))?;
                if key == "language" {
                    language = s;
                }
                String::new()
            }
        };
        if !rest.trim().is_empty() {
            return Err(format!("line {}: unexpected text after the list", n + 1));
        }
    }
    if open.is_some() {
        return Err("unclosed list".into());
    }
    Ok((language, lists))
}

/// Cuts a `#` comment that is not inside a quoted string.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Reads quoted, comma-separated items until `]`. Returns whether the list
/// closed and the text after `]`.
fn read_items(mut s: &str, items: &mut Vec<String>) -> Result<(bool, String), String> {
    loop {
        s = s.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if s.is_empty() {
            return Ok((false, String::new()));
        }
        if let Some(rest) = s.strip_prefix(']') {
            return Ok((true, rest.to_string()));
        }
        let body = s.strip_prefix('"').ok_or("expected a quoted string")?;
        let end = body.find('"').ok_or("unclosed string")?;
        items.push(body[..end].to_string());
        s = &body[end + 1..];
    }
}

fn unquote(s: &str) -> Option<String> {
    s.strip_prefix('"')?.strip_suffix('"').map(str::to_string)
}

fn all() -> &'static Lists {
    static ALL: OnceLock<Lists> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all = Lists::new();
        for (code, src) in PACKS {
            // The packs are compiled in and covered by tests, so a parse
            // error is a build mistake, not a user input.
            let (language, lists) = parse(src).unwrap_or_else(|e| panic!("lang/{code}.toml: {e}"));
            debug_assert_eq!(language, *code);
            for (key, items) in lists {
                let list = all.entry(key).or_default();
                for item in items {
                    if !list.contains(&item) {
                        list.push(item);
                    }
                }
            }
        }
        all
    })
}

/// Every key of table `table` (without the `table.` prefix) with its list,
/// merged over all packs. Sorted by key so callers see a stable order.
pub(crate) fn table(table: &str) -> Vec<(&'static str, &'static [String])> {
    let prefix = format!("{table}.");
    let mut out: Vec<_> = all()
        .iter()
        .filter_map(|(k, v)| k.strip_prefix(prefix.as_str()).map(|name| (name, v.as_slice())))
        .collect();
    out.sort_by_key(|(name, _)| *name);
    out
}

/// List `key` merged over all packs (empty when no pack has it).
pub(crate) fn list(key: &str) -> &'static [String] {
    all().get(key).map_or(&[], Vec::as_slice)
}

/// True when `word` (already lower case where the caller wants that) is in
/// list `key` of any pack.
pub(crate) fn contains(key: &str, word: &str) -> bool {
    all().get(key).is_some_and(|list| list.iter().any(|w| w == word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pack_parses_with_its_code_and_lists() {
        for (code, src) in PACKS {
            let (language, lists) = parse(src).unwrap();
            assert_eq!(language, *code);
            for key in [FILLERS, STUTTER_WORDS, ABBREVIATIONS, LOWERCASE_ABBREVIATIONS, NOUN_MARKERS, AMBIGUOUS_MARKS, KEEP_FINAL_STOP] {
                let list = lists.get(key).unwrap_or_else(|| panic!("{code}: no {key}"));
                assert!(!list.is_empty(), "{code}: empty {key}");
                assert!(list.iter().all(|w| !w.is_empty() && w.to_lowercase() == *w), "{code}: {key} {list:?}");
            }
        }
    }

    #[test]
    fn lookups_see_every_language() {
        assert!(contains(FILLERS, "öö"));
        assert!(contains(FILLERS, "um"));
        assert!(contains(ABBREVIATIONS, "kft"));
        assert!(contains(ABBREVIATIONS, "e.g"));
        assert!(contains(KEEP_FINAL_STOP, "stb"));
        assert!(contains(KEEP_FINAL_STOP, "approx"));
        assert!(!contains(KEEP_FINAL_STOP, "no"));
        assert!(contains(STUTTER_WORDS, "hogy"));
        assert!(!contains(FILLERS, "hát"));
        assert!(!contains("no_such_list", "a"));
    }

    #[test]
    fn number_and_date_words_load_whole() {
        // Same sizes as the arrays they replaced in backtrack.rs.
        let sizes = [(NUMBER_PARTS, 28), (NUMBER_ENDINGS, 36), (NUMBER_WORDS, 20), (WHEN_STEMS, 25), (WHEN_WORDS, 15)];
        for (key, n) in sizes {
            assert_eq!(list(key).len(), n, "{key}");
        }
        assert_eq!(list(NUMBER_ENDINGS).last().map(String::as_str), Some(""));
        assert!(contains(WHEN_WORDS, "ma") && contains(WHEN_WORDS, "tomorrow"));
        assert!(list("no_such_list").is_empty());
    }

    #[test]
    fn parser_handles_comments_multiline_lists_and_errors() {
        let src = "language = \"xx\" # code\nfillers = [\n  \"a#b\", # hash inside quotes\n  \"c\",\n]\nother = [\"d\"]\n";
        let (language, lists) = parse(src).unwrap();
        assert_eq!(language, "xx");
        assert_eq!(lists["fillers"], ["a#b", "c"]);
        assert_eq!(lists["other"], ["d"]);
        assert!(parse("fillers = [\"a\"").is_err());
        assert!(parse("fillers = [a]").is_err());
        assert!(parse("just text").is_err());
        assert!(parse("fillers = [\"a\"] x").is_err());
    }

    #[test]
    fn tables_prefix_their_keys() {
        let src = "language = \"xx\"\ntop = [\"a\"]\n[marks] # spoken\ncomma = [\"vessző\", \"vesző\"]\n[paired]\nopen_paren = [\"nyitó zárójel\"]\n";
        let (language, lists) = parse(src).unwrap();
        assert_eq!(language, "xx");
        assert_eq!(lists["top"], ["a"]);
        assert_eq!(lists["marks.comma"], ["vessző", "vesző"]);
        assert_eq!(lists["paired.open_paren"], ["nyitó zárójel"]);
        assert!(parse("[]").is_err());
        assert!(parse("[a b]").is_err());
        assert!(parse("[marks").is_err());
        let marks: Vec<_> = table("marks").into_iter().map(|(name, _)| name).collect();
        assert!(marks.contains(&"comma") && marks.contains(&"paragraph"), "{marks:?}");
        assert!(table("marks").iter().any(|(_, list)| list.iter().any(|p| p == "new line")));
    }
}
