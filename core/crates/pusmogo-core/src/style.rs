//! Writing styles per app: a deterministic pass after `cleanup::clean`.
//!
//! The app reports the frontmost app (bundle id and window title) at key-down;
//! `category_for` maps it to a category (personal messages, work messages,
//! email, other) and the user picks a style per category: Formal (capitals and
//! full punctuation), Casual (no final full stop on a one-sentence message) or
//! Very casual (lower-case start, no final full stop). Email also gets layout:
//! a greeting and a sign-off the user actually said, each on its own line.
//! A sign-off ends with a comma only when a name follows ("Üdvözlettel,\nPéter");
//! a bare closing keeps a spoken "!" and otherwise ends without a mark.
//!
//! Nothing here invents words. Register (te / Ön) is not a rule and stays a
//! possible future polish option.

use crate::engine::Language;
use crate::lang;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Other = 0,
    Personal = 1,
    Work = 2,
    Email = 3,
    /// Code editors, IDEs and terminals: always the code style (`code.rs`).
    Code = 4,
}

impl Category {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Category::Personal,
            2 => Category::Work,
            3 => Category::Email,
            4 => Category::Code,
            _ => Category::Other,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "other" => Some(Category::Other),
            "personal" => Some(Category::Personal),
            "work" => Some(Category::Work),
            "email" => Some(Category::Email),
            "code" => Some(Category::Code),
            _ => None,
        }
    }

    /// Defaults: personal messages Casual, everything else Formal.
    pub fn default_style(self) -> Style {
        match self {
            Category::Personal => Style::Casual,
            _ => Style::Formal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Formal = 0,
    Casual = 1,
    VeryCasual = 2,
}

impl Style {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Style::Casual,
            2 => Style::VeryCasual,
            _ => Style::Formal,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
            "formal" => Some(Style::Formal),
            "casual" => Some(Style::Casual),
            "very_casual" => Some(Style::VeryCasual),
            _ => None,
        }
    }
}

const PERSONAL_APPS: &[&str] = &[
    "net.whatsapp.WhatsApp",
    "desktop.WhatsApp",
    "ru.keepcoder.Telegram",
    "org.telegram.desktop",
    "com.facebook.archon",
    "com.facebook.archon.developerID",
    "org.whispersystems.signal-desktop",
    "com.hnc.Discord",
    "com.apple.MobileSMS",
];

const WORK_APPS: &[&str] = &["com.tinyspeck.slackmacgap", "com.microsoft.teams", "com.microsoft.teams2"];

const EMAIL_APPS: &[&str] = &[
    "com.apple.mail",
    "com.microsoft.Outlook",
    "com.readdle.smartemail-Mac",
    "com.readdle.SparkDesktop",
    "com.superhuman.electron",
];

/// Code editors, IDEs and terminals. JetBrains IDEs share a bundle id prefix.
const CODE_APPS: &[&str] = &[
    "com.microsoft.VSCode",
    "com.microsoft.VSCodeInsiders",
    "com.vscodium",
    "com.todesktop.230313mzl4w4u92",
    "com.exafunction.windsurf",
    "com.apple.dt.Xcode",
    "dev.zed.Zed",
    "dev.zed.Zed-Preview",
    "com.sublimetext.4",
    "com.panic.Nova",
    "com.google.android.studio",
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "dev.warp.Warp-Stable",
    "net.kovidgoyal.kitty",
    "org.alacritty",
    "com.github.wez.wezterm",
];
const CODE_APP_PREFIXES: &[&str] = &["com.jetbrains."];

const BROWSERS: &[&str] = &[
    "com.apple.Safari",
    "com.apple.SafariTechnologyPreview",
    "com.google.Chrome",
    "com.google.Chrome.beta",
    "company.thebrowser.Browser",
    "com.microsoft.edgemac",
    "org.mozilla.firefox",
    "com.brave.Browser",
    "com.operasoftware.Opera",
    "com.vivaldi.Vivaldi",
];

/// Web apps recognised by a whole part of a browser window title
/// ("Inbox (3) - someone@example.com - Gmail", "Mail - Name - Outlook").
const WEB_APPS: &[(&str, Category)] = &[
    ("Gmail", Category::Email),
    ("Outlook", Category::Email),
    ("Google Chat", Category::Work),
    ("Slack", Category::Work),
    ("Microsoft Teams", Category::Work),
    ("WhatsApp", Category::Personal),
    ("Messenger", Category::Personal),
    ("Telegram", Category::Personal),
    ("Discord", Category::Personal),
];

/// The category for the frontmost app. `overrides` holds the user's own
/// mapping, one `bundle.id<TAB>category` per line (category names as in
/// `Category::from_name`); it wins over the built-in lists.
pub fn category_for(bundle_id: &str, window_title: &str, overrides: &str) -> Category {
    let bundle = bundle_id.trim();
    for line in overrides.lines() {
        if let Some((id, cat)) = line.split_once('\t') {
            if id.trim() == bundle {
                if let Some(c) = Category::from_name(cat) {
                    return c;
                }
            }
        }
    }
    if PERSONAL_APPS.contains(&bundle) {
        return Category::Personal;
    }
    if WORK_APPS.contains(&bundle) {
        return Category::Work;
    }
    if EMAIL_APPS.contains(&bundle) {
        return Category::Email;
    }
    if CODE_APPS.contains(&bundle) || CODE_APP_PREFIXES.iter().any(|p| bundle.starts_with(p)) {
        return Category::Code;
    }
    if BROWSERS.contains(&bundle) {
        return web_app_category(window_title);
    }
    Category::Other
}

fn web_app_category(title: &str) -> Category {
    let mut t = title.replace('\u{a0}', " ");
    for sep in [" — ", " – ", " | "] {
        t = t.replace(sep, " - ");
    }
    for part in t.split(" - ").map(str::trim) {
        if let Some((_, cat)) = WEB_APPS.iter().find(|(name, _)| part == *name) {
            return *cat;
        }
    }
    Category::Other
}

/// Applies the style for `category` to text already cleaned by `clean()`.
/// The code category ignores `style`: it always gets `code::apply`.
pub fn apply(text: &str, category: Category, style: Style, lang: Language) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    if category == Category::Email {
        if let Some(laid_out) = email_layout(text, style, lang) {
            return laid_out;
        }
    }
    if let Some(list) = crate::lists::layout(text, category, style) {
        return list;
    }
    if category == Category::Code {
        return crate::code::apply(text);
    }
    style_body(text, style)
}

pub(crate) fn style_body(text: &str, style: Style) -> String {
    match style {
        Style::Formal => end_paragraphs(text),
        Style::Casual => {
            if is_one_sentence(text) {
                drop_final_stop(text)
            } else {
                end_paragraphs(text)
            }
        }
        Style::VeryCasual => lower_start(&drop_final_stop(text)),
    }
}

fn is_terminal(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…')
}

/// Formal: every paragraph (before a blank line, and at the end) that ends in a
/// letter or digit gets a full stop. Single line breaks (lists) are left alone.
fn end_paragraphs(text: &str) -> String {
    text.split("\n\n").map(|p| add_stop(p.trim_end())).collect::<Vec<_>>().join("\n\n")
}

fn add_stop(p: &str) -> String {
    match p.chars().last() {
        Some(c) if c.is_alphanumeric() => format!("{p}."),
        _ => p.to_string(),
    }
}

/// One sentence: no sentence mark before the last character and no line break.
fn is_one_sentence(text: &str) -> bool {
    if text.contains('\n') {
        return false;
    }
    let body = text.trim_end_matches(is_terminal);
    !body.contains(['!', '?', '…']) && !body.split(". ").nth(1).is_some_and(|_| true)
}

fn drop_final_stop(text: &str) -> String {
    let Some(body) = text.strip_suffix('.') else {
        return text.to_string();
    };
    if body.ends_with('.') {
        return text.to_string(); // "..." stays
    }
    let last = body.rsplit(|c: char| c.is_whitespace()).next().unwrap_or("");
    let bare = last.trim_start_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    if bare.is_empty() || bare.contains('.') || bare.chars().all(|c| c.is_ascii_digit()) || lang::contains(lang::KEEP_FINAL_STOP, &bare) {
        return text.to_string();
    }
    body.to_string()
}

/// Very casual: lower-case first letter, unless the first word looks like an
/// acronym or brand (two capitals: "PDF", "OpenAI") or is the English "I".
fn lower_start(text: &str) -> String {
    let first = text.split_whitespace().next().unwrap_or("");
    let word = first.trim_end_matches(|c: char| !c.is_alphanumeric());
    if word.chars().filter(|c| c.is_uppercase()).count() >= 2 || word == "I" || word.starts_with("I'") || word.starts_with("I’") {
        return text.to_string();
    }
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// --- Email layout ---------------------------------------------------------

/// Greeting openers. `needs_name`: "Kedves" or "Dear" alone is not a greeting.
struct Opener {
    words: &'static str,
    needs_name: bool,
    hungarian: bool,
}

const OPENERS: &[Opener] = &[
    Opener { words: "jó napot kívánok", needs_name: false, hungarian: true },
    Opener { words: "jó reggelt kívánok", needs_name: false, hungarian: true },
    Opener { words: "jó napot", needs_name: false, hungarian: true },
    Opener { words: "jó reggelt", needs_name: false, hungarian: true },
    Opener { words: "sziasztok", needs_name: false, hungarian: true },
    Opener { words: "szia", needs_name: false, hungarian: true },
    Opener { words: "szervusz", needs_name: false, hungarian: true },
    Opener { words: "szervusztok", needs_name: false, hungarian: true },
    Opener { words: "helló", needs_name: false, hungarian: true },
    Opener { words: "kedves", needs_name: true, hungarian: true },
    Opener { words: "tisztelt", needs_name: true, hungarian: true },
    Opener { words: "good morning", needs_name: false, hungarian: false },
    Opener { words: "good afternoon", needs_name: false, hungarian: false },
    Opener { words: "hello", needs_name: false, hungarian: false },
    Opener { words: "hi", needs_name: false, hungarian: false },
    Opener { words: "hey", needs_name: false, hungarian: false },
    Opener { words: "dear", needs_name: true, hungarian: false },
];

/// Words that count as the addressee even in lower case ("Tisztelt Kovács úr",
/// "Hi all", "Kedves kollégák").
const ADDRESS_WORDS: &[&str] = &[
    "úr", "asszony", "hölgyem", "uram", "kolléga", "kollégák", "kollégáim", "mindenki", "csapat", "barátaim", "mr", "mrs", "ms", "dr",
    "there", "all", "everyone", "everybody", "team", "guys", "folks", "sir", "madam",
];

/// Address words written with a capital in a greeting.
const TITLES: &[&str] = &["úr", "asszony", "hölgyem", "uram", "mr", "mrs", "ms", "dr", "sir", "madam"];

/// Title abbreviations: their full stop is part of the name, not the end of the
/// greeting ("Tisztelt Dr. Kovács Péter", "Dear Mr. Smith", "Kedves ifj. Nagy János").
const TITLE_ABBREVIATIONS: &[&str] = &["dr", "mr", "mrs", "ms", "prof", "ifj", "id"];

/// Capitalised words that start the body, not a name ("Szia Péter Köszönöm…").
const NOT_NAMES: &[&str] = &[
    "köszönöm", "köszi", "köszönjük", "kérlek", "kérem", "remélem", "holnap", "ma", "ezt", "ez", "az", "a", "egy", "i", "thanks", "thank",
    "the", "this", "we", "just", "hope", "please",
];

fn bare_word(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-').to_lowercase()
}

/// Words that join two addressees in a greeting ("Kedves Anna és Péter!").
const NAME_JOINERS: &[&str] = &["és", "and"];

/// A word that can be part of the addressee: a capitalised name, an address word or a title abbreviation.
fn name_like(word: &str) -> bool {
    let bare = bare_word(word);
    let core = word.trim_end_matches([',', '!', '.', ':', ';']);
    let is_name = core.chars().next().is_some_and(char::is_uppercase) && !NOT_NAMES.contains(&bare.as_str());
    let is_abbreviation = TITLE_ABBREVIATIONS.contains(&bare.as_str()) && word.ends_with('.');
    is_name || ADDRESS_WORDS.contains(&bare.as_str()) || is_abbreviation
}

/// After "Anna,": true when the words up to the greeting's mark are more names
/// joined by "és"/"and" ("Péter és Éva!"), so the comma lists names rather than
/// ending the greeting. Anything else (a body word, no joiner, no closing mark) is false.
fn continues_name_list(rest: &str) -> bool {
    let mut in_person = 0;
    let mut joined = false;
    for word in rest.split_whitespace() {
        if NAME_JOINERS.contains(&word) {
            // "Peter, and Kim": a joiner may follow a listing comma, but only once.
            if joined {
                return false;
            }
            joined = true;
            in_person = 0;
            continue;
        }
        if !name_like(word) {
            return false;
        }
        in_person += 1;
        if in_person > 4 {
            return false;
        }
        let core = word.trim_end_matches([',', '!', '.', ':', ';']);
        if core.len() < word.len() && !(word.len() == core.len() + 1 && word.ends_with('.') && TITLE_ABBREVIATIONS.contains(&bare_word(word).as_str())) {
            if word.ends_with(',') && !joined {
                in_person = 0;
                continue;
            }
            return joined;
        }
    }
    false
}

/// Splits a greeting off the start of `text`: returns (greeting line, rest).
fn split_greeting(text: &str, lang: Language) -> Option<(String, String)> {
    let first_line_end = text.find('\n').unwrap_or(text.len());
    let line = &text[..first_line_end];
    let lower = line.to_lowercase();
    let opener = OPENERS.iter().find(|o| {
        lower.starts_with(o.words) && lower[o.words.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric())
    })?;
    // Lower-casing keeps byte lengths for every opener (all their letters map 1:1).
    let mut rest = &line[opener.words.len()..];
    let opener_text = capitalise(&line[..opener.words.len()]);

    let mut name: Vec<String> = Vec::new();
    let mut in_person = 0;
    let mut punctuated = false;
    loop {
        let trimmed = rest.trim_start();
        if let Some(after) = trimmed.strip_prefix([',', '!', '.', ':', ';']) {
            rest = after;
            punctuated = true;
            break;
        }
        let Some(word) = trimmed.split_whitespace().next() else {
            rest = trimmed;
            break;
        };
        let bare = bare_word(word);
        // "Anna és Péter": a joiner stays in the greeting only when another addressee follows.
        if NAME_JOINERS.contains(&word) && !name.is_empty() {
            let after = &trimmed[word.len()..];
            if after.split_whitespace().next().is_some_and(name_like) && name.len() < 12 {
                name.push(word.to_string());
                in_person = 0;
                rest = after;
                continue;
            }
            // "Kedves Anna és a csapat": unclear where the greeting ends, so no layout.
            return None;
        }
        let core = word.trim_end_matches([',', '!', '.', ':', ';']);
        let is_address = ADDRESS_WORDS.contains(&bare.as_str());
        let is_name = core.chars().next().is_some_and(char::is_uppercase) && !NOT_NAMES.contains(&bare.as_str());
        let is_abbreviation = TITLE_ABBREVIATIONS.contains(&bare.as_str()) && word.len() == core.len() + 1 && word.ends_with('.');
        if in_person >= 4 || !(is_address || is_name || is_abbreviation) {
            rest = trimmed;
            break;
        }
        if is_abbreviation {
            // "ifj." and "id." stay lower case; "Dr.", "Mr.", "Prof." take a capital.
            let keep = if bare == "ifj" || bare == "id" { word.to_string() } else { capitalise(word) };
            name.push(keep);
            in_person += 1;
            rest = &trimmed[word.len()..];
            continue;
        }
        // Titles take a capital in a greeting ("Kovács Úr"); group words do not ("Hi all").
        let mut kept = if TITLES.contains(&bare.as_str()) { capitalise(core) } else { core.to_string() };
        rest = &trimmed[word.len()..];
        in_person += 1;
        // "Anna, Péter és Éva!": a comma inside a list of names does not end the greeting.
        if word.ends_with(',') && word.len() == core.len() + 1 && continues_name_list(rest) {
            kept.push(',');
            name.push(kept);
            in_person = 0;
            continue;
        }
        name.push(kept);
        if core.len() < word.len() {
            punctuated = true;
            break;
        }
    }
    // A title with no name after it ("Kedves Dr. Köszönöm…") ends the greeting without its dot.
    if let Some(last) = name.last_mut() {
        if TITLE_ABBREVIATIONS.contains(&bare_word(last).as_str()) {
            last.truncate(last.trim_end_matches('.').len());
        }
    }
    if opener.needs_name && name.is_empty() {
        return None;
    }
    if name.is_empty() && !punctuated && !rest.trim().is_empty() {
        return None; // "Szia holnap jövök": no clear greeting
    }
    let hungarian = match lang {
        Language::Hungarian => true,
        Language::English => false,
        Language::Auto => opener.hungarian,
    };
    let mark = if hungarian { '!' } else { ',' };
    let mut greeting = opener_text;
    if !name.is_empty() {
        greeting.push(' ');
        greeting.push_str(&name.join(" "));
    }
    greeting.push(mark);
    let rest = format!("{}{}", rest.trim_start(), &text[first_line_end..]);
    Some((greeting, rest.trim().to_string()))
}

/// Closers. `clear`: may follow a comma inside the last sentence
/// ("…holnap küldöm, üdvözlettel, Péter").
const CLOSERS: &[(&str, bool)] = &[
    ("üdvözlettel", true),
    ("baráti üdvözlettel", true),
    ("üdvözletel", true),
    ("baráti üdvözletel", true),
    ("tisztelettel", true),
    ("köszönettel", true),
    ("barátsággal", true),
    ("üdv", false),
    ("köszönöm szépen", false),
    ("köszönöm", false),
    ("köszi", false),
    ("puszi", false),
    ("best regards", true),
    ("kind regards", true),
    ("warm regards", true),
    ("regards", true),
    ("sincerely", true),
    ("best wishes", true),
    ("all the best", false),
    ("thank you", false),
    ("thanks", false),
    ("cheers", false),
    ("best", false),
];

/// Misspellings Whisper writes for a closer (a TTS "Üdvözlettel" came out as
/// "Üdvözletel"): accepted as the closer and typed correctly.
const CLOSER_FIXES: &[(&str, &str)] = &[("üdvözletel", "üdvözlettel")];

/// The closer as typed: the spoken words, with a known misspelling corrected.
fn closer_text(spoken: &str) -> String {
    let lower = spoken.to_lowercase();
    for (wrong, right) in CLOSER_FIXES {
        if let Some(prefix) = lower.strip_suffix(wrong) {
            // Keep the user's casing of any words before the misspelt one.
            let keep = &spoken[..prefix.len()];
            return capitalise(&format!("{keep}{right}"));
        }
    }
    capitalise(spoken)
}

/// Splits a sign-off off the end of `text`: returns (body, sign-off lines).
fn split_signoff(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim_end();
    let without_end = trimmed.trim_end_matches(is_terminal);
    let tail_start = last_sentence_start(without_end);
    let tail = &trimmed[tail_start..];
    // Candidate starts: the start of the last sentence, then after each comma.
    let mut starts = vec![(0usize, true)];
    for (i, _) in tail.match_indices(',') {
        starts.push((i + 1, false));
    }
    for (start, at_sentence) in starts {
        let candidate = tail[start..].trim_start();
        let lower = candidate.to_lowercase();
        // Longest closer first so "köszönöm szépen" wins over "köszönöm".
        let mut closers: Vec<&(&str, bool)> = CLOSERS.iter().collect();
        closers.sort_by_key(|(w, _)| std::cmp::Reverse(w.len()));
        for (words, clear) in closers {
            if !(at_sentence || *clear) || !lower.starts_with(words) {
                continue;
            }
            let after = &candidate[words.len()..];
            if after.chars().next().is_some_and(char::is_alphanumeric) {
                continue;
            }
            let Some(name) = signoff_name(after) else { continue };
            if *words == "best" && name.is_empty() {
                continue; // "Best." alone is too unsure
            }
            let body_end = tail_start + start;
            let body = trimmed[..body_end].trim_end().trim_end_matches(',').trim_end();
            if body.is_empty() {
                return None;
            }
            let mut sign = closer_text(&candidate[..words.len()]);
            if name.is_empty() {
                // No name: the comma would dangle. Keep a spoken "!", add nothing.
                if after.trim_start().starts_with('!') {
                    sign.push('!');
                }
            } else {
                sign.push_str(",\n");
                sign.push_str(&name);
            }
            return Some((body.to_string(), sign));
        }
    }
    None
}

/// Start of the last sentence: after the last '.', '!', '?' or line break, skipping
/// the full stop of a title abbreviation ("Üdvözlettel, Dr. Kovács Péter").
fn last_sentence_start(text: &str) -> usize {
    let mut end = text.len();
    while let Some(i) = text[..end].rfind(['.', '!', '?', '\n']) {
        let before = &text[..i];
        let word = before.rsplit(|c: char| !c.is_alphabetic()).next().unwrap_or("");
        let is_title = text[i..].starts_with('.') && TITLE_ABBREVIATIONS.contains(&word.to_lowercase().as_str());
        if !is_title {
            return i + 1;
        }
        end = i;
    }
    0
}

/// After a closer: nothing, or a mark and up to three capitalised name words
/// (four with a title abbreviation: "Dr. Kovács Péter").
fn signoff_name(after: &str) -> Option<String> {
    let rest = after.trim_start_matches([',', '!', '.', ':', ' ']).trim_end_matches(is_terminal).trim();
    if rest.is_empty() {
        return Some(String::new());
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    let titled = words.first().is_some_and(|w| w.ends_with('.') && TITLE_ABBREVIATIONS.contains(&bare_word(w).as_str()));
    if words.len() > 3 + usize::from(titled) || !words.iter().skip(usize::from(titled)).all(|w| w.chars().next().is_some_and(char::is_uppercase) && w.chars().all(|c| c.is_alphabetic() || c == '-' || c == '.')) {
        return None;
    }
    let mut name: Vec<String> = words.iter().map(|w| w.to_string()).collect();
    if titled {
        name[0] = capitalise(&name[0]); // "dr. Kovács Péter" → "Dr. Kovács Péter"
    }
    Some(name.join(" "))
}

fn email_layout(text: &str, style: Style, lang: Language) -> Option<String> {
    let (greeting, rest) = match split_greeting(text, lang) {
        Some((g, r)) => (Some(g), r),
        None => (None, text.to_string()),
    };
    let (body, signoff) = match split_signoff(&rest) {
        Some((b, s)) => (b, Some(s)),
        None => (rest, None),
    };
    // Layout needs a body: a greeting or a sign-off said on its own stays as said.
    if body.trim().is_empty() || (greeting.is_none() && signoff.is_none()) {
        return None;
    }
    let mut parts = Vec::new();
    if let Some(g) = greeting {
        parts.push(g);
    }
    if !body.is_empty() {
        // A body is more than one line once laid out: Casual keeps its stops.
        let styled = match (crate::lists::layout(&body, Category::Email, style), style) {
            (Some(list), _) => list,
            (None, Style::Formal | Style::Casual) => end_paragraphs(&capitalise(&body)),
            (None, Style::VeryCasual) => style_body(&body, style),
        };
        parts.push(styled);
    }
    if let Some(s) = signoff {
        parts.push(s);
    }
    Some(parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HU: Language = Language::Hungarian;
    const EN: Language = Language::English;

    #[test]
    fn final_stop_kept_only_after_pack_abbreviations() {
        assert_eq!(drop_final_stop("Hozd a tejet, kenyeret stb."), "Hozd a tejet, kenyeret stb.");
        assert_eq!(drop_final_stop("Apples, pears etc."), "Apples, pears etc.");
        assert_eq!(drop_final_stop("Megyek a Kft."), "Megyek a Kft.");
        assert_eq!(drop_final_stop("I said no."), "I said no");
        assert_eq!(drop_final_stop("Jövök."), "Jövök");
    }

    #[test]
    fn maps_apps_to_categories() {
        assert_eq!(category_for("net.whatsapp.WhatsApp", "", ""), Category::Personal);
        assert_eq!(category_for("com.apple.MobileSMS", "", ""), Category::Personal);
        assert_eq!(category_for("com.tinyspeck.slackmacgap", "", ""), Category::Work);
        assert_eq!(category_for("com.microsoft.Outlook", "", ""), Category::Email);
        assert_eq!(category_for("com.apple.mail", "", ""), Category::Email);
        assert_eq!(category_for("com.apple.TextEdit", "", ""), Category::Other);
        assert_eq!(category_for("com.microsoft.VSCode", "", ""), Category::Code);
        assert_eq!(category_for("com.jetbrains.intellij", "", ""), Category::Code);
        assert_eq!(category_for("com.mitchellh.ghostty", "", ""), Category::Code);
    }

    #[test]
    fn code_category_uses_the_code_style() {
        let c = Category::Code;
        assert_eq!(apply("Camel case user ID.", c, Style::Formal, EN), "userId");
        assert_eq!(apply("Futtasd le a teszteket.", c, Style::Casual, HU), "futtasd le a teszteket");
        // The user mapping can move an editor out of the code category.
        assert_eq!(category_for("com.microsoft.VSCode", "", "com.microsoft.VSCode\tother"), Category::Other);
    }

    #[test]
    fn maps_web_apps_by_window_title() {
        assert_eq!(category_for("com.google.Chrome", "Inbox (3) - someone@example.com - Gmail", ""), Category::Email);
        assert_eq!(category_for("com.apple.Safari", "Mail - Anna Kovács - Outlook", ""), Category::Email);
        assert_eq!(category_for("company.thebrowser.Browser", "Google Chat", ""), Category::Work);
        assert_eq!(category_for("org.mozilla.firefox", "Gmail — Mozilla Firefox", ""), Category::Email);
        // A word inside a title is not the web app.
        assert_eq!(category_for("com.google.Chrome", "Economic Outlook 2027 report", ""), Category::Other);
        assert_eq!(category_for("com.apple.TextEdit", "Gmail", ""), Category::Other);
    }

    #[test]
    fn user_mapping_wins() {
        let overrides = "com.apple.TextEdit\temail\nnet.whatsapp.WhatsApp\twork\nbad line\ncom.x\tnonsense";
        assert_eq!(category_for("com.apple.TextEdit", "", overrides), Category::Email);
        assert_eq!(category_for("net.whatsapp.WhatsApp", "", overrides), Category::Work);
        assert_eq!(category_for("com.x", "", overrides), Category::Other);
    }

    #[test]
    fn defaults_per_category() {
        assert_eq!(Category::Personal.default_style(), Style::Casual);
        assert_eq!(Category::Work.default_style(), Style::Formal);
        assert_eq!(Category::Email.default_style(), Style::Formal);
        assert_eq!(Category::Other.default_style(), Style::Formal);
    }

    #[test]
    fn formal_ends_paragraphs() {
        let o = Category::Other;
        assert_eq!(apply("Holnap háromkor találkozunk", o, Style::Formal, HU), "Holnap háromkor találkozunk.");
        assert_eq!(apply("Mikor jössz?", o, Style::Formal, HU), "Mikor jössz?");
        assert_eq!(apply("Első rész\n\nMásodik rész", o, Style::Formal, HU), "Első rész.\n\nMásodik rész.");
        // A list keeps its lines bare.
        assert_eq!(apply("Tej\nKenyér\nVaj", o, Style::Formal, HU), "Tej\nKenyér\nVaj.");
        assert_eq!(apply("See you at 3 pm", o, Style::Formal, EN), "See you at 3 pm.");
    }

    #[test]
    fn casual_drops_the_stop_of_one_sentence() {
        let p = Category::Personal;
        assert_eq!(apply("Ok, de ma nem érek rá, holnap igen.", p, Style::Casual, HU), "Ok, de ma nem érek rá, holnap igen");
        assert_eq!(apply("Mikor jössz?", p, Style::Casual, HU), "Mikor jössz?");
        assert_eq!(apply("Jó ötlet!", p, Style::Casual, HU), "Jó ötlet!");
        assert_eq!(apply("Megjöttem. Hol vagy?", p, Style::Casual, HU), "Megjöttem. Hol vagy?");
        assert_eq!(apply("Megjöttem. Mindjárt megyek", p, Style::Casual, HU), "Megjöttem. Mindjárt megyek.");
        assert_eq!(apply("Sounds good.", p, Style::Casual, EN), "Sounds good");
    }

    #[test]
    fn casual_keeps_abbreviations_ellipses_and_ordinals() {
        let p = Category::Personal;
        assert_eq!(apply("Hozz tejet, kenyeret stb.", p, Style::Casual, HU), "Hozz tejet, kenyeret stb.");
        assert_eq!(apply("Várj...", p, Style::Casual, HU), "Várj...");
        assert_eq!(apply("Ez a 3.", p, Style::Casual, HU), "Ez a 3.");
        assert_eq!(apply("Back in the U.S.", p, Style::Casual, EN), "Back in the U.S.");
    }

    #[test]
    fn very_casual_lowers_the_start() {
        let p = Category::Personal;
        assert_eq!(apply("Mindjárt ott vagyok.", p, Style::VeryCasual, HU), "mindjárt ott vagyok");
        assert_eq!(apply("Megjöttem. Hol vagy?", p, Style::VeryCasual, HU), "megjöttem. Hol vagy?");
        assert_eq!(apply("PDF-ben küldöm.", p, Style::VeryCasual, HU), "PDF-ben küldöm");
        assert_eq!(apply("OpenAI announced it.", p, Style::VeryCasual, EN), "OpenAI announced it");
        assert_eq!(apply("I'm on my way.", p, Style::VeryCasual, EN), "I'm on my way");
        assert_eq!(apply("Élvezd!", p, Style::VeryCasual, HU), "élvezd!");
    }

    #[test]
    fn email_greeting_and_signoff_hungarian() {
        let e = Category::Email;
        assert_eq!(
            apply("Kedves Anna, köszönöm a tegnapi megbeszélést. Holnap küldöm a jelentést. Üdvözlettel, Péter.", e, Style::Formal, HU),
            "Kedves Anna!\n\nKöszönöm a tegnapi megbeszélést. Holnap küldöm a jelentést.\n\nÜdvözlettel,\nPéter"
        );
        assert_eq!(
            apply("Szia Péter! Holnap háromkor jó lesz. Köszi!", e, Style::Formal, HU),
            "Szia Péter!\n\nHolnap háromkor jó lesz.\n\nKöszi!"
        );
        assert_eq!(
            apply("Tisztelt Kovács úr, a számlát csatoltam. Tisztelettel, Nagy Éva", e, Style::Formal, HU),
            "Tisztelt Kovács Úr!\n\nA számlát csatoltam.\n\nTisztelettel,\nNagy Éva"
        );
        assert_eq!(
            apply("Sziasztok, a meeting áthelyezve csütörtökre", e, Style::Formal, HU),
            "Sziasztok!\n\nA meeting áthelyezve csütörtökre."
        );
        // A closer after a comma inside the last sentence.
        assert_eq!(
            apply("Kedves Anna! Holnap küldöm, üdvözlettel, Péter", e, Style::Formal, HU),
            "Kedves Anna!\n\nHolnap küldöm.\n\nÜdvözlettel,\nPéter"
        );
    }

    #[test]
    fn email_signoff_keeps_a_titled_name() {
        let e = Category::Email;
        assert_eq!(
            apply("Kedves Anna, a jelentést csatoltam. Üdvözlettel, Dr. Kovács Péter", e, Style::Formal, HU),
            "Kedves Anna!\n\nA jelentést csatoltam.\n\nÜdvözlettel,\nDr. Kovács Péter"
        );
        assert_eq!(
            apply("Hi Alex, the deck is attached. Best regards, Dr. Sam Lee.", e, Style::Formal, EN),
            "Hi Alex,\n\nThe deck is attached.\n\nBest regards,\nDr. Sam Lee"
        );
        assert_eq!(
            apply("Kedves Anna! Holnap küldöm. Tisztelettel, dr. Nagy Éva", e, Style::Formal, HU),
            "Kedves Anna!\n\nHolnap küldöm.\n\nTisztelettel,\nDr. Nagy Éva"
        );
        // A title in the body is not a sentence end, and a title alone is no name.
        assert_eq!(
            apply("Kedves Anna! Dr. Kovács holnap jön. Üdvözlettel, Péter", e, Style::Formal, HU),
            "Kedves Anna!\n\nDr. Kovács holnap jön.\n\nÜdvözlettel,\nPéter"
        );
    }

    #[test]
    fn email_greeting_and_signoff_english() {
        let e = Category::Email;
        assert_eq!(
            apply("Hi Alex, thanks for the update. I'll send the deck tomorrow. Best regards, John", e, Style::Formal, EN),
            "Hi Alex,\n\nThanks for the update. I'll send the deck tomorrow.\n\nBest regards,\nJohn"
        );
        assert_eq!(
            apply("Dear Ms Smith, please find the invoice attached. Kind regards.", e, Style::Formal, EN),
            "Dear Ms Smith,\n\nPlease find the invoice attached.\n\nKind regards"
        );
        assert_eq!(apply("Hi all, the call moves to Friday. Thanks!", e, Style::Formal, EN), "Hi all,\n\nThe call moves to Friday.\n\nThanks!");
        assert_eq!(
            apply("The numbers look fine. Best, Sam", e, Style::Formal, EN),
            "The numbers look fine.\n\nBest,\nSam"
        );
    }

    #[test]
    fn email_greeting_keeps_a_title_abbreviation() {
        let e = Category::Email;
        assert_eq!(
            apply("Tisztelt Dr. Kovács Péter, köszönöm a levelét.", e, Style::Formal, HU),
            "Tisztelt Dr. Kovács Péter!\n\nKöszönöm a levelét."
        );
        assert_eq!(
            apply("Tisztelt dr. Kovács Péter, köszönöm a levelét.", e, Style::Formal, HU),
            "Tisztelt Dr. Kovács Péter!\n\nKöszönöm a levelét."
        );
        assert_eq!(apply("Kedves ifj. Nagy János! Holnap jövök.", e, Style::Formal, HU), "Kedves ifj. Nagy János!\n\nHolnap jövök.");
        assert_eq!(apply("Dear Mr. Smith, see you on Friday.", e, Style::Formal, EN), "Dear Mr. Smith,\n\nSee you on Friday.");
        assert_eq!(apply("Hi Ms. Lee, the file is attached.", e, Style::Formal, EN), "Hi Ms. Lee,\n\nThe file is attached.");
        assert_eq!(apply("Dear Prof. Brown, thank you for the notes.", e, Style::Formal, EN), "Dear Prof. Brown,\n\nThank you for the notes.");
        // A title with no name after it loses its dot rather than doubling the mark.
        assert_eq!(apply("Kedves Dr. Köszönöm a levelét.", e, Style::Formal, HU), "Kedves Dr!\n\nKöszönöm a levelét.");
    }

    #[test]
    fn email_greeting_keeps_several_names() {
        let e = Category::Email;
        assert_eq!(apply("Kedves Anna és Péter! Holnap jövök.", e, Style::Formal, HU), "Kedves Anna és Péter!\n\nHolnap jövök.");
        assert_eq!(apply("Dear Anna and Peter, see you soon.", e, Style::Formal, EN), "Dear Anna and Peter,\n\nSee you soon.");
        assert_eq!(apply("Kedves Anna, Péter és Éva! Holnap jövök.", e, Style::Formal, HU), "Kedves Anna, Péter és Éva!\n\nHolnap jövök.");
        assert_eq!(apply("Hi Anna, Peter, and Kim, the call is at three.", e, Style::Formal, EN), "Hi Anna, Peter, and Kim,\n\nThe call is at three.");
        assert_eq!(
            apply("Tisztelt Dr. Kovács Péter és Nagy Éva, a számlát csatoltam.", e, Style::Formal, HU),
            "Tisztelt Dr. Kovács Péter és Nagy Éva!\n\nA számlát csatoltam."
        );
        assert_eq!(apply("Szia Anna és csapat, kész a terv.", e, Style::Formal, HU), "Szia Anna és csapat!\n\nKész a terv.");
        // A comma that is not followed by a joined list of names still ends the greeting.
        assert_eq!(apply("Szia Anna, Péter holnap jön.", e, Style::Formal, HU), "Szia Anna!\n\nPéter holnap jön.");
        // Unclear where the greeting ends: no layout rather than a cut name.
        assert_eq!(apply("Kedves Anna és a csapat, holnap jövök.", e, Style::Formal, HU), "Kedves Anna és a csapat, holnap jövök.");
    }

    #[test]
    fn email_signoff_without_a_name_has_no_dangling_comma() {
        let e = Category::Email;
        assert_eq!(
            apply("Kedves Anna, nagyon hálás vagyok. Köszönöm", e, Style::Formal, HU),
            "Kedves Anna!\n\nNagyon hálás vagyok.\n\nKöszönöm"
        );
        assert_eq!(apply("Hi Alex, see you Friday. Thank you.", e, Style::Formal, EN), "Hi Alex,\n\nSee you Friday.\n\nThank you");
        assert_eq!(apply("Szia Kata! Este jövök. Puszi, Sára", e, Style::Formal, HU), "Szia Kata!\n\nEste jövök.\n\nPuszi,\nSára");
        assert_eq!(apply("Szia Kata! Este jövök. Puszi!", e, Style::Formal, HU), "Szia Kata!\n\nEste jövök.\n\nPuszi!");
        // "Puszi" inside a sentence is not a sign-off.
        assert_eq!(apply("Este jövök, puszi neked is.", e, Style::Formal, HU), "Este jövök, puszi neked is.");
    }

    #[test]
    fn email_accepts_a_misspelt_signoff() {
        let e = Category::Email;
        assert_eq!(
            apply("Kedves Anna, holnap küldöm a jelentést. Üdvözletel, Péter.", e, Style::Formal, HU),
            "Kedves Anna!\n\nHolnap küldöm a jelentést.\n\nÜdvözlettel,\nPéter"
        );
        assert_eq!(
            apply("Holnap küldöm, baráti üdvözletel, Péter", e, Style::Formal, HU),
            "Holnap küldöm.\n\nBaráti üdvözlettel,\nPéter"
        );
        // Not a closer when more words follow that are not a name.
        assert_eq!(
            apply("Üdvözletel küldöm a csapatnak.", e, Style::Formal, HU),
            "Üdvözletel küldöm a csapatnak."
        );
    }

    #[test]
    fn email_never_invents_a_greeting_or_signoff() {
        let e = Category::Email;
        assert_eq!(apply("A számlát csatoltam", e, Style::Formal, HU), "A számlát csatoltam.");
        // "Kedves" alone is an adjective, not a greeting.
        assert_eq!(apply("Kedves tőled, köszönöm a segítséget.", e, Style::Formal, HU), "Kedves tőled, köszönöm a segítséget.");
        // "Köszönöm" followed by more words is a sentence, not a sign-off.
        assert_eq!(apply("Köszönöm a segítséget.", e, Style::Formal, HU), "Köszönöm a segítséget.");
        assert_eq!(apply("Thanks for the help.", e, Style::Formal, EN), "Thanks for the help.");
        // "Hi" without a mark or a name is not a clear greeting.
        assert_eq!(apply("Szia holnap jövök", e, Style::Formal, HU), "Szia holnap jövök.");
        // A sign-off alone (no body) stays as said.
        assert_eq!(apply("Köszönöm, Péter.", e, Style::Formal, HU), "Köszönöm, Péter.");
        assert_eq!(apply("The best.", e, Style::Formal, EN), "The best.");
        // A greeting alone keeps its own mark.
        assert_eq!(apply("Helló.", e, Style::Formal, HU), "Helló.");
        assert_eq!(apply("Hi Alex", e, Style::Formal, EN), "Hi Alex.");
    }

    #[test]
    fn email_keeps_spoken_paragraphs() {
        let e = Category::Email;
        assert_eq!(
            apply("Kedves Anna!\n\nAz első rész kész\n\nA második jövő héten jön. Üdvözlettel, Péter", e, Style::Formal, HU),
            "Kedves Anna!\n\nAz első rész kész.\n\nA második jövő héten jön.\n\nÜdvözlettel,\nPéter"
        );
    }

    #[test]
    fn other_categories_ignore_email_layout() {
        assert_eq!(
            apply("Szia Péter, holnap háromkor találkozunk.", Category::Personal, Style::Casual, HU),
            "Szia Péter, holnap háromkor találkozunk"
        );
        assert_eq!(apply("Szia Péter, holnap háromkor találkozunk.", Category::Work, Style::Formal, HU), "Szia Péter, holnap háromkor találkozunk.");
    }

    #[test]
    fn empty_text_stays_empty() {
        assert_eq!(apply("  ", Category::Email, Style::Formal, HU), "");
    }
}
