//! Lists by voice: a dictation that clearly marks two or more items becomes a
//! list, laid out for the app it goes into.
//!
//! Markers, Hungarian and English (the dictated language does not matter, as
//! in the cleanup):
//! - ordinals at the start of a clause: "először …, másodszor …, harmadszor …",
//!   "first …, second …", optionally closed by "végül" / "finally";
//! - numbers at the start of a clause followed by a comma or colon, as Whisper
//!   writes a spoken count: "egy, tej, kettő, kenyér", "one: …, two: …",
//!   also "number one, …, number two, …" and written "1. …, 2. …";
//! - bullets anywhere: "bullet point …", "new bullet …", "listapont …".
//!
//! Ordinals and numbers must run 1, 2, 3 … without a gap, and every item must
//! hold a word, so a lone "Először elmentem a boltba" or a plain count
//! ("egy, kettő, három") stays prose. Ordinals also narrate ("Először nem
//! értettem, másodszor már igen"), so an ordinal item may not open with a
//! word such as "is", "pedig", "már" or "also", and a two-item ordinal list
//! needs two words per item ("Elsőként Péter jön, másodikként Anna" stays
//! prose). Nothing here adds or drops a word other
//! than the markers themselves and a joining "és" / "and" between items.

use crate::style::{Category, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Numbered,
    Bullets,
}

#[derive(Clone, Copy)]
enum Family {
    /// Ordinal words, item k uses `words[k]`; "végül" / "finally" may close.
    Ordinals(&'static [&'static [&'static str]]),
    /// Number words, item k uses `words[k]`; a bare one needs "," or ":" after.
    Numbers,
    /// Written digits "1." "2)" at the start of a clause.
    Digits,
    /// The same marker before every item, anywhere in the text.
    Bullets,
}

const HU_ORDINALS: &[&[&str]] = &[
    &["először is", "először", "elsőként", "elsősorban"],
    &["másodszor", "másodsorban", "másodszorban", "másodikként"],
    &["harmadszor", "harmadsorban", "harmadikként"],
    &["negyedszer", "negyedsorban", "negyedikként"],
    &["ötödször", "ötödsorban", "ötödikként"],
    &["hatodszor", "hatodsorban"],
    &["hetedszer", "hetedsorban"],
    &["nyolcadszor", "nyolcadsorban"],
    &["kilencedszer", "kilencedsorban"],
    &["tizedszer", "tizedsorban"],
];

const EN_ORDINALS: &[&[&str]] = &[
    &["first of all", "firstly", "first"],
    &["secondly", "second"],
    &["thirdly", "third"],
    &["fourthly", "fourth"],
    &["fifthly", "fifth"],
    &["sixthly", "sixth"],
    &["seventh"],
    &["eighth"],
    &["ninth"],
    &["tenth"],
];

/// Said after "number"; like the bare number words, they need "," or ":" after.
const NUMBERED: &[&str] = &["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];
const NUMBER_WORDS: &[&[&str]] = &[
    &["egy", "one"],
    &["kettő", "two"],
    &["három", "three"],
    &["négy", "four"],
    &["öt", "five"],
    &["hat", "six"],
    &["hét", "seven"],
    &["nyolc", "eight"],
    &["kilenc", "nine"],
    &["tíz", "ten"],
];

const BULLETS: &[&str] = &["new bullet point", "bullet point", "new bullet", "új listapont", "listapont", "felsorolásjel"];

/// Close an ordinal list of at least two items.
const FINALS: &[&str] = &["és végül", "és végezetül", "végül", "végezetül", "and finally", "and lastly", "finally", "lastly"];

/// A word right after an ordinal that makes it narration, not a list marker:
/// "másodszor is visszamentem", "először nem értettem", "second time I got it".
const PROSE_AFTER_ORDINAL: &[&str] = &[
    "is", "pedig", "sem", "se", "már", "még", "nem", "viszont", "azonban", "also", "too", "time", "place",
];

/// A word that may join two items; it belongs to neither.
const JOINERS: &[&str] = &["és", "and", "majd", "aztán", "then"];

const FAMILIES: &[Family] = &[Family::Ordinals(HU_ORDINALS), Family::Ordinals(EN_ORDINALS), Family::Numbers, Family::Digits, Family::Bullets];

struct List {
    kind: Kind,
    intro: String,
    items: Vec<String>,
    outro: String,
}

/// Lays `text` (already cleaned) out as a list for `category`, or None when it
/// does not clearly mark two or more items. Numbered lists get "1." and bullets
/// "•" (code: "-"); chat apps get one line per item without marks.
pub fn layout(text: &str, category: Category, style: Style) -> Option<String> {
    let list = detect(text)?;
    let code = category == Category::Code;
    let shape = |s: &str| -> String {
        if code {
            crate::code::terse(s)
        } else if style == Style::VeryCasual {
            lower_first(s)
        } else {
            upper_first(s)
        }
    };
    let mut out = String::new();
    if !list.intro.is_empty() {
        let mut intro = shape(&list.intro);
        if intro.chars().last().is_some_and(char::is_alphanumeric) {
            intro.push(':');
        }
        out.push_str(&intro);
        out.push('\n');
    }
    let lines: Vec<String> = list
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mark = match (category, list.kind) {
                (Category::Personal | Category::Work, _) => String::new(),
                (Category::Code, Kind::Bullets) => "- ".to_string(),
                (_, Kind::Bullets) => "• ".to_string(),
                (_, Kind::Numbered) => format!("{}. ", i + 1),
            };
            format!("{mark}{}", shape(item))
        })
        .collect();
    out.push_str(&lines.join("\n"));
    if !list.outro.is_empty() {
        let outro = if code { crate::code::apply(&list.outro) } else { crate::style::style_body(&list.outro, style) };
        out.push_str("\n\n");
        out.push_str(&outro);
    }
    Some(out)
}

fn detect(text: &str) -> Option<List> {
    let text = text.trim();
    let mut best: Option<(Kind, Vec<(usize, usize)>)> = None;
    for family in FAMILIES {
        let Some(marks) = find_marks(text, *family) else { continue };
        if matches!(family, Family::Ordinals(_)) && !ordinal_list(text, &marks) {
            continue;
        }
        let better = match &best {
            None => true,
            Some((_, b)) => marks.len() > b.len() || (marks.len() == b.len() && marks[0].0 < b[0].0),
        };
        if better {
            let kind = if matches!(family, Family::Bullets) { Kind::Bullets } else { Kind::Numbered };
            best = Some((kind, marks));
        }
    }
    let (kind, marks) = best?;
    let intro = clean_piece(&text[..marks[0].0]);
    let mut items = Vec::new();
    for (i, &(_, content)) in marks.iter().enumerate() {
        let end = marks.get(i + 1).map_or(text.len(), |m| m.0);
        items.push(&text[content..end]);
    }
    let last = items.pop()?;
    let (last, outro) = split_outro(last);
    items.push(last);
    let items: Vec<String> = items.into_iter().map(clean_piece).collect();
    if items.iter().any(|i| !i.chars().any(char::is_alphabetic) || is_number_word(i)) {
        return None;
    }
    Some(List { kind, intro, items, outro: outro.trim().to_string() })
}

/// Ordinals are common in narration ("Először nem értettem, másodszor már
/// igen", "Elsőként Péter jön, másodikként Anna"), so an ordinal list needs
/// items that read as list entries: none may open with a word that ties the
/// ordinal into the sentence, and a two-item list needs two words per item
/// besides an article.
fn ordinal_list(text: &str, marks: &[(usize, usize)]) -> bool {
    let items: Vec<&str> = marks
        .iter()
        .enumerate()
        .map(|(i, &(_, content))| &text[content..marks.get(i + 1).map_or(text.len(), |m| m.0)])
        .collect();
    let words = |s: &str| -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
    };
    let last = items.len() - 1;
    items.iter().enumerate().all(|(i, item)| {
        let item = if i == last { split_outro(item).0 } else { item };
        let ws = words(item);
        let opens_prose = ws.first().is_some_and(|w| PROSE_AFTER_ORDINAL.contains(&w.as_str()));
        let content = ws.iter().filter(|w| !matches!(w.as_str(), "a" | "az" | "an" | "the")).count();
        !opens_prose && (items.len() > 2 || content >= 2)
    })
}

/// Marker spans (start including a joining word, content start) of one family,
/// at least two of them, or None.
fn find_marks(text: &str, family: Family) -> Option<Vec<(usize, usize)>> {
    let mut marks: Vec<(usize, usize)> = Vec::new();
    let mut closed = false;
    for (p, _) in text.char_indices() {
        if closed {
            break;
        }
        if marks.last().is_some_and(|m| p < m.1) || !word_start(text, p) {
            continue;
        }
        let anywhere = matches!(family, Family::Bullets);
        if !anywhere && !clause_start(text, p) {
            continue;
        }
        let k = marks.len();
        let mut starts = vec![p];
        if !anywhere {
            if let Some(end) = match_words(text, p, JOINERS) {
                if text[end..].starts_with(' ') {
                    starts.push(end + 1);
                }
            }
        }
        for q in starts {
            if let Some(end) = marker_at(text, q, family, k) {
                marks.push((p, end));
                break;
            }
            if matches!(family, Family::Ordinals(_)) && k >= 2 {
                if let Some(end) = match_words(text, q, FINALS) {
                    marks.push((p, end));
                    closed = true;
                    break;
                }
            }
        }
    }
    (marks.len() >= 2).then_some(marks)
}

/// End of item `k`'s marker at `q`, if there is one.
fn marker_at(text: &str, q: usize, family: Family, k: usize) -> Option<usize> {
    let rest = &text[q..];
    match family {
        Family::Ordinals(words) => match_words(text, q, words.get(k)?),
        Family::Bullets => match_words(text, q, BULLETS),
        Family::Numbers => {
            if let Some(end) = match_words(text, q, &["number"]) {
                let after = text[end..].strip_prefix(' ')?;
                let start = text.len() - after.len();
                let end = match_words(text, start, &[NUMBERED.get(k)?])?;
                return text[end..].starts_with([',', ':']).then_some(end);
            }
            let end = match_words(text, q, NUMBER_WORDS.get(k)?)?;
            text[end..].starts_with([',', ':']).then_some(end)
        }
        Family::Digits => {
            let n = (k + 1).to_string();
            let after = rest.strip_prefix(n.as_str())?;
            let mut chars = after.chars();
            let mark = chars.next()?;
            (matches!(mark, '.' | ')' | ':') && chars.next().is_some_and(char::is_whitespace)).then_some(q + n.len() + 1)
        }
    }
}

/// End of the first of `words` found at `q` (case-insensitive, whole words).
fn match_words(text: &str, q: usize, words: &[&str]) -> Option<usize> {
    let rest = &text[q..];
    for w in words {
        let mut end = 0;
        let mut ok = true;
        let mut chars = rest.char_indices();
        for wc in w.chars() {
            match chars.next() {
                Some((i, c)) if c.to_lowercase().eq(wc.to_lowercase()) => end = i + c.len_utf8(),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && !rest[end..].chars().next().is_some_and(char::is_alphanumeric) {
            return Some(q + end);
        }
    }
    None
}

fn word_start(text: &str, p: usize) -> bool {
    !text[..p].chars().last().is_some_and(char::is_alphanumeric)
}

/// The start of the text, or a word after ",", ".", ":", ";", "!", "?" or a line break.
fn clause_start(text: &str, p: usize) -> bool {
    let before = &text[..p];
    if before.trim().is_empty() {
        return true;
    }
    if before.ends_with('\n') {
        return true;
    }
    before.ends_with(char::is_whitespace) && before.trim_end().ends_with([',', '.', ':', ';', '!', '?', '\n'])
}

/// Splits the last item at its first sentence end followed by a capital:
/// "hazamegyek. Utána alszom." → ("hazamegyek.", "Utána alszom.").
fn split_outro(s: &str) -> (&str, &str) {
    for (i, c) in s.char_indices() {
        if matches!(c, '.' | '!' | '?') {
            let after = &s[i + c.len_utf8()..];
            if after.starts_with(char::is_whitespace) && after.trim_start().chars().next().is_some_and(char::is_uppercase) {
                return (&s[..i + c.len_utf8()], after);
            }
        }
    }
    (s, "")
}

/// Trims marks and joining words around an item or the intro; keeps "?" and "!".
fn clean_piece(s: &str) -> String {
    let mut s = s.trim().trim_start_matches(|c: char| matches!(c, ',' | ':' | ';' | '-' | '–') || c.is_whitespace());
    loop {
        let before = s;
        s = s.trim_end_matches(|c: char| matches!(c, ',' | ';' | '.' | ':') || c.is_whitespace());
        if let Some(cut) = s.rfind(' ') {
            if match_words(s, cut + 1, JOINERS) == Some(s.len()) {
                s = &s[..cut];
            }
        }
        if s == before {
            break;
        }
    }
    s.to_string()
}

fn is_number_word(s: &str) -> bool {
    NUMBER_WORDS.iter().any(|ws| ws.iter().any(|w| s.eq_ignore_ascii_case(w) || s.to_lowercase() == *w))
}

fn upper_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Lower-cases the first letter unless the first word looks like an acronym.
fn lower_first(s: &str) -> String {
    let first = s.split_whitespace().next().unwrap_or("");
    if first.chars().filter(|c| c.is_uppercase()).count() >= 2 || first == "I" {
        return s.to_string();
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn other(text: &str) -> Option<String> {
        layout(text, Category::Other, Style::Formal)
    }

    #[test]
    fn hungarian_ordinals_make_a_numbered_list() {
        assert_eq!(
            other("Három dolgot kell tenned. Először vedd meg a tejet, másodszor hívd fel anyát, harmadszor vidd el a csomagot.").as_deref(),
            Some("Három dolgot kell tenned:\n1. Vedd meg a tejet\n2. Hívd fel anyát\n3. Vidd el a csomagot")
        );
        assert_eq!(
            other("Elsőként nézd át a szerződést, másodsorban küldd el Péternek, és végül írd alá.").as_deref(),
            Some("1. Nézd át a szerződést\n2. Küldd el Péternek\n3. Írd alá")
        );
    }

    #[test]
    fn spoken_numbers_make_a_numbered_list() {
        assert_eq!(other("A bevásárló lista, egy, tej, kettő, kenyér, három, alma.").as_deref(), Some("A bevásárló lista:\n1. Tej\n2. Kenyér\n3. Alma"));
        assert_eq!(
            other("Number one, fix the login bug. Number two, update the docs.").as_deref(),
            Some("1. Fix the login bug\n2. Update the docs")
        );
        assert_eq!(other("Teendők: 1. tej, 2. kenyér.").as_deref(), Some("Teendők:\n1. Tej\n2. Kenyér"));
    }

    #[test]
    fn english_ordinals_and_an_outro() {
        assert_eq!(
            other("We need three things. First, book the room. Second, send the invite. Third, order lunch. Then we are done.").as_deref(),
            Some("We need three things:\n1. Book the room\n2. Send the invite\n3. Order lunch\n\nThen we are done.")
        );
    }

    #[test]
    fn bullets_follow_the_app() {
        let text = "Bullet point milk bullet point bread bullet point eggs";
        assert_eq!(other(text).as_deref(), Some("• Milk\n• Bread\n• Eggs"));
        assert_eq!(layout(text, Category::Email, Style::Formal).as_deref(), Some("• Milk\n• Bread\n• Eggs"));
        assert_eq!(layout(text, Category::Code, Style::Formal).as_deref(), Some("- milk\n- bread\n- eggs"));
        assert_eq!(layout(text, Category::Personal, Style::Casual).as_deref(), Some("Milk\nBread\nEggs"));
        assert_eq!(layout("Listapont alma, listapont körte.", Category::Other, Style::Formal).as_deref(), Some("• Alma\n• Körte"));
    }

    #[test]
    fn chat_apps_get_one_line_per_item() {
        assert_eq!(
            layout("Először vedd meg a tejet, másodszor hívd fel anyát.", Category::Work, Style::Formal).as_deref(),
            Some("Vedd meg a tejet\nHívd fel anyát")
        );
        assert_eq!(
            layout("Először vedd meg a tejet, másodszor hívd fel anyát.", Category::Personal, Style::VeryCasual).as_deref(),
            Some("vedd meg a tejet\nhívd fel anyát")
        );
    }

    #[test]
    fn ordinary_prose_stays_prose() {
        for text in [
            "Először elmentem a boltba, aztán hazamentem.",
            "Először elmentem a boltba.",
            "First I went home and then I slept.",
            "First, I went home. Then I slept.",
            "Egy, kettő, három.",
            "One, two, three, four.",
            "Ez az első alkalom, a második pedig jövő héten lesz.",
            "Másodszor is megkérdezte, először nem válaszoltam.",
            "Egy, kettő.",
            "He came first, second was Anna.",
            "The bullet point is clear.",
            "Kettő, egy, nulla.",
            "2. kerület, 1. emelet.",
            "A 2024. évben 1. helyezett lett.",
        ] {
            assert_eq!(other(text), None, "{text}");
        }
    }

    /// Narration with two ordinals or "number N": none of it is a list.
    #[test]
    fn two_ordinal_narration_stays_prose() {
        for text in [
            "Először nem értettem, másodszor már igen.",
            "Először is köszönöm, másodszor pedig gratulálok.",
            "Először elmentem a boltba, aztán másodszor is visszamentem.",
            "Harmadszor is megpróbálom, először nem ment, másodszor sem.",
            "Elsőként Péter jön, másodikként Anna.",
            "Először nem tetszett, másodszor viszont nagyon.",
            "Először féltem, másodszor már nem.",
            "Először Budapestre megyünk, másodszor Bécsbe.",
            "Először ő nyert, másodszor én.",
            "Először is elnézést, másodszor is elnézést.",
            "Elsősorban a pénz számít, másodsorban az idő.",
            "Először nem sikerült, másodszor sem, harmadszor is elbuktam.",
            "Először még nem tudtam, másodszor már tudtam.",
            "Először azt hittem, hogy viccel. Másodszor már elhittem.",
            "Először is, nagyon örülök. Másodszor pedig köszönöm a meghívást.",
            "Elsőként érkezett, másodikként távozott.",
            "Először elutasították, másodszor azonban elfogadták.",
            "Először se hívott, másodszor se.",
            "First of all, thank you, and second, congratulations.",
            "First, thanks. Second, congratulations.",
            "First I was confused, second time I got it.",
            "First time I tried it failed, second time it worked.",
            "He finished first, second was Anna.",
            "Firstly, sorry. Secondly, thanks.",
            "Először elmentem a boltba, másodszor a postára.",
            "First of all, welcome. Secondly, enjoy.",
            "She came first, and second place went to Tom.",
            "Number one priority is speed, number two is size.",
            "Number one fan, number two fan.",
            "He was number one in the class and number two in the school.",
        ] {
            assert_eq!(other(text), None, "{text}");
            assert_eq!(layout(text, Category::Personal, Style::Casual), None, "{text}");
        }
    }

    #[test]
    fn two_item_ordinal_lists_still_work() {
        assert_eq!(
            other("Először vedd meg a tejet, másodszor hívd fel anyát.").as_deref(),
            Some("1. Vedd meg a tejet\n2. Hívd fel anyát")
        );
        assert_eq!(
            other("Először meg kell venni a tejet, másodszor fel kell hívni anyát.").as_deref(),
            Some("1. Meg kell venni a tejet\n2. Fel kell hívni anyát")
        );
        assert_eq!(other("First, book the room. Second, send the invite.").as_deref(), Some("1. Book the room\n2. Send the invite"));
    }
}
