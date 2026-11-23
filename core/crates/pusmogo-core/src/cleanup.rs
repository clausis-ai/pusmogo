//! Instant, rule-based cleanup that runs after every engine and before the
//! optional LLM polish. Hungarian-aware: removes hesitation sounds (ö, öö, hmm)
//! but never real discourse words like "hát", "szóval", "tehát", "ugye".

use crate::engine::Language;
use crate::lang;
use std::sync::OnceLock;

const TRAILING_PUNCT: &[char] = &[',', '.', '!', '?', ';', ':', '…'];

/// Quotes and brackets that may wrap a dictionary word ("(kubernetes)").
// Also a trailing-off "..." / "…" glued to the next word and a hashtag
// ("...kubernetes", "#pusmogo"), kept in front of the spelling.
const OPENING_WRAP: &[char] = &['(', '[', '"', '\'', '„', '“', '‘', '«', '»', '.', '…', '#'];
const CLOSING_WRAP: &[char] = &[')', ']', '"', '\'', '”', '“', '’', '»', '«'];

/// Personal dictionary: spoken/misheard form → wanted spelling.
/// Matching is case-insensitive and on whole words.
#[derive(Default, Clone, Debug)]
pub struct Dictionary {
    entries: Vec<(String, String)>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a replacement. `from` may be the same as `to` to only bias the engine
    /// prompt (e.g. "Clausis") without a text fix.
    pub fn add(&mut self, from: &str, to: &str) {
        self.entries.push((from.to_lowercase(), to.to_string()));
    }

    /// Whether `word` is part of a wanted spelling.
    pub fn has_word(&self, word: &str) -> bool {
        let word = word.to_lowercase();
        self.entries.iter().any(|(_, to)| to.split_whitespace().any(|w| w.to_lowercase() == word))
    }

    /// The vocabulary hint for the engine prompt: the wanted spellings,
    /// deduplicated, as one labelled sentence ("… Gyakori szavak: A, B.").
    /// Whisper reads the prompt as the text before the clip; a labelled,
    /// full-stopped list kept more names right and cost less general accuracy
    /// than a bare comma list (docs/bench/dict-01.md). Empty if no entries.
    pub fn prompt(&self) -> String {
        Self::format_prompt(&self.spellings())
    }

    /// Like `prompt`, but no longer than `max_tokens` as counted by `count`
    /// (the engine's tokenizer). Whisper keeps only the last 224 prompt tokens
    /// and cuts the rest from the front, label and all; with an 80-entry
    /// dictionary that cost 1.1 WER points on FLEURS (docs/bench/dict-02.md).
    /// So the oldest spellings are left out of the prompt here instead; their
    /// replacements in `clean` still apply. `count` returning `None` means no
    /// tokenizer: the full prompt is returned.
    pub fn prompt_fitting(&self, max_tokens: usize, count: impl Fn(&str) -> Option<usize>) -> String {
        self.fit(&[], max_tokens, count).0
    }

    /// Like `prompt_fitting`, with `extra` spellings (context hints from the
    /// text around the cursor) listed after the dictionary's, and the label in
    /// the dictation language. Extras go last because Whisper weighs the end of
    /// the prompt most and the budget cuts the oldest dictionary spellings
    /// first. An English label kept one more name right on the English name
    /// set (Kowalczyk; 11.4 → 10.0 % WER) and changed nothing on the English
    /// synthetic set; Hungarian and Auto keep the Hungarian label.
    pub fn prompt_fitting_with(&self, extra: &[String], lang: Language, max_tokens: usize, count: impl Fn(&str) -> Option<usize>) -> String {
        self.fit_labelled(extra, prompt_label(lang), max_tokens, count).0
    }

    /// How many spellings `prompt_fitting` leaves out, so the Dictionary tab
    /// can say that the oldest entries no longer steer the engine. `None`
    /// when `count` has no tokenizer (stub engine, model not loaded).
    pub fn prompt_left_out(&self, max_tokens: usize, count: impl Fn(&str) -> Option<usize>) -> Option<usize> {
        count(&self.prompt())?;
        Some(self.fit(&[], max_tokens, count).1)
    }

    /// The fitted prompt and the number of oldest spellings it skips.
    fn fit(&self, extra: &[String], max_tokens: usize, count: impl Fn(&str) -> Option<usize>) -> (String, usize) {
        self.fit_labelled(extra, prompt_label(Language::Hungarian), max_tokens, count)
    }

    fn fit_labelled(&self, extra: &[String], label: &str, max_tokens: usize, count: impl Fn(&str) -> Option<usize>) -> (String, usize) {
        let mut words = self.spellings();
        let own = words.len();
        for w in extra {
            if !words.iter().any(|d| d.to_lowercase() == w.to_lowercase()) {
                words.push(w.as_str());
            }
        }
        let mut skip = 0;
        loop {
            let prompt = Self::format_prompt_labelled(&words[skip..], label);
            match count(&prompt) {
                Some(n) if n > max_tokens && skip < words.len() => skip += 1,
                _ => return (prompt, skip.min(own)),
            }
        }
    }

    /// Wanted spellings in the order they were added, without duplicates.
    fn spellings(&self) -> Vec<&str> {
        let mut words: Vec<&str> = Vec::new();
        for (_, to) in &self.entries {
            if !words.contains(&to.as_str()) {
                words.push(to);
            }
        }
        words
    }

    fn format_prompt(words: &[&str]) -> String {
        Self::format_prompt_labelled(words, prompt_label(Language::Hungarian))
    }

    fn format_prompt_labelled(words: &[&str], label: &str) -> String {
        if words.is_empty() {
            return String::new();
        }
        let list = words.join(", ");
        let stop = if list.ends_with(['.', '!', '?']) { "" } else { "." };
        format!("{label} {list}{stop}")
    }

    fn lookup(&self, word: &str) -> Option<&str> {
        let lower = word.to_lowercase();
        self.entries
            .iter()
            .find(|(from, _)| *from == lower)
            .map(|(_, to)| to.as_str())
    }

    /// Like `lookup`, for a word as Whisper writes it: wrapped in quotes or
    /// brackets ("(kubernetes)", "„pusmogo”") or carrying a Hungarian suffix
    /// after a hyphen ("pusmogo-t", "iphone-nal"). Returns the whole token
    /// with the spelling put in, wrapping and suffix kept ("(Kubernetes)",
    /// "Pusmogo-t"). A word that matches as a whole wins over its stem, so
    /// an entry "wi-fi" still applies.
    fn lookup_wrapped(&self, token: &str) -> Option<String> {
        if let Some(to) = self.lookup(token) {
            return Some(to.to_string());
        }
        let inner = token.trim_start_matches(OPENING_WRAP);
        let open = &token[..token.len() - inner.len()];
        let body = inner.trim_end_matches(CLOSING_WRAP);
        let close = &inner[body.len()..];
        let (word, punct) = split_trailing_punct(body);
        if word.is_empty() {
            return None;
        }
        // Words joined by a slash ("kubernetes/pusmogo") are matched one by
        // one. Links and paths ("https://…", "example.com/pusmogo") stay.
        if word.contains('/') && !word.contains("://") {
            let parts: Vec<&str> = word.split('/').collect();
            if parts.iter().all(|p| !p.is_empty() && !p.contains('.')) {
                let fixed: Vec<String> = parts
                    .iter()
                    .map(|p| self.lookup_wrapped(p).unwrap_or_else(|| p.to_string()))
                    .collect();
                if fixed.iter().zip(&parts).any(|(f, p)| f != p) {
                    let spelled = fixed.join("/");
                    return Some(format!("{open}{spelled}{}{close}", dedupe_stop(&spelled, punct)));
                }
            }
            return None;
        }
        let spelled = match self.lookup(word) {
            Some(to) if !open.is_empty() || !close.is_empty() => to.to_string(),
            Some(_) => return None,
            None => {
                let (stem, tail) = split_kept_tail(word)?;
                format!("{}{tail}", self.lookup(stem)?)
            }
        };
        Some(format!("{open}{spelled}{}{close}", dedupe_stop(&spelled, punct)))
    }

    /// The longest multi-word entry ("open ai" → "OpenAI") whose words match
    /// `tokens` from the start, as the text to type and the number of tokens
    /// it covers. Inner words must be bare; "open, ai" is two separate words.
    /// The first word may open a quote or bracket and the last may close one,
    /// end with punctuation or carry a Hungarian suffix after a hyphen, all
    /// kept around the spelling ("(open ai)" → "(OpenAI)", "open ai-t" →
    /// "OpenAI-t"), like `lookup_wrapped` does for one word.
    fn lookup_phrase(&self, tokens: &[&str]) -> Option<(String, usize)> {
        let mut best: Option<(String, usize)> = None;
        for (from, to) in &self.entries {
            let words: Vec<&str> = from.split_whitespace().collect();
            let n = words.len();
            if n < 2 || n > tokens.len() || best.as_ref().is_some_and(|(_, used)| *used >= n) {
                continue;
            }
            let first = tokens[0].trim_start_matches(OPENING_WRAP);
            let open = &tokens[0][..tokens[0].len() - first.len()];
            if first.to_lowercase() != words[0] {
                continue;
            }
            if !(1..n - 1).all(|k| tokens[k].to_lowercase() == words[k]) {
                continue;
            }
            let (last, punct) = split_trailing_punct(tokens[n - 1]);
            let inner = last.trim_end_matches(CLOSING_WRAP);
            let close = &last[inner.len()..];
            let (word, inner_punct) = split_trailing_punct(inner);
            let suffix = if word.to_lowercase() == words[n - 1] {
                ""
            } else {
                match split_kept_tail(word) {
                    Some((stem, tail)) if stem.to_lowercase() == words[n - 1] => tail,
                    _ => continue,
                }
            };
            let spelled = format!("{to}{suffix}");
            let after = format!("{inner_punct}{close}{punct}");
            let text = format!("{open}{spelled}{}", dedupe_stop(&spelled, &after));
            best = Some((text, n));
        }
        best
    }
}

/// The label that opens the engine prompt. Whisper reads the prompt as the
/// text before the clip, so it is written in the dictation language. The
/// Hungarian label starts with a short Hungarian sentence: a list of English
/// names or terms after a bare label made short Hungarian clips come out as
/// English respellings ("Szia" → "See ya"); the sentence won back most of
/// that (short-phrase WER 41.3 → 36.6 % with terminal hints, no prompt 35.1 %;
/// docs/bench/prompt-02.md).
pub(crate) const HU_PROMPT_LABEL: &str = "Ez egy magyar nyelvű jegyzet. Gyakori szavak:";

fn prompt_label(lang: Language) -> &'static str {
    match lang {
        Language::English => "Common words:",
        Language::Hungarian | Language::Auto => HU_PROMPT_LABEL,
    }
}

/// A word's stem and the tail a dictionary spelling keeps: a Hungarian
/// suffix after a hyphen ("pusmogo-t" → "pusmogo", "-t"), an English
/// possessive with either apostrophe ("open ai's" → "ai", "'s") or a version
/// after a hyphen, kept verbatim with anything after it ("gpt-4o" → "gpt",
/// "-4o"; "gpt-3.5-öt" → "gpt", "-3.5-öt").
fn split_kept_tail(word: &str) -> Option<(&str, &str)> {
    if let Some((stem, version)) = word.split_once('-') {
        // A possessive may close the version ("gpt-4's").
        let core = version.strip_suffix("'s").or_else(|| version.strip_suffix("’s")).unwrap_or(version);
        let is_version = core.starts_with(|c: char| c.is_ascii_digit())
            && core.chars().count() <= 12
            && core.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-');
        if !stem.is_empty() && is_version {
            return Some((stem, &word[stem.len()..]));
        }
    }
    for possessive in ["'s", "’s"] {
        if let Some(stem) = word.strip_suffix(possessive) {
            if !stem.is_empty() {
                return Some((stem, &word[stem.len()..]));
            }
        }
    }
    let (stem, suffix) = word.rsplit_once('-')?;
    let is_suffix = (1..=6).contains(&suffix.chars().count()) && suffix.chars().all(char::is_lowercase);
    is_suffix.then(|| (stem, &word[stem.len()..]))
}

fn split_trailing_punct(token: &str) -> (&str, &str) {
    let core = token.trim_end_matches(TRAILING_PUNCT);
    (core, &token[core.len()..])
}

/// The punctuation after a dictionary spelling, without a full stop the
/// spelling already ends with ("Clausis Kft." + "." stays "Clausis Kft.").
fn dedupe_stop<'a>(spelling: &str, punct: &'a str) -> &'a str {
    if spelling.ends_with('.') {
        punct.strip_prefix('.').unwrap_or(punct)
    } else {
        punct
    }
}

fn is_filler(core: &str) -> bool {
    // An all-caps word of two or more letters is an acronym ("HM", "ER", "UH",
    // "HMM"), never a hesitation sound.
    let letters = core.chars().filter(|c| c.is_alphabetic()).count();
    if letters >= 2 && core.chars().all(|c| !c.is_lowercase()) {
        return false;
    }
    let lower = core.to_lowercase();
    lang::contains(lang::FILLERS, &lower)
}

/// Puts cleaned text on one line for a single-line field (search box, web
/// `<input>`, subject line). Such fields drop line breaks on insert, which
/// would glue "jövök.\nHozol" into "jövök.Hozol"; a space keeps the words apart.
pub fn single_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Cleans raw engine output.
pub fn clean(raw: &str, dict: &Dictionary) -> String {
    let tokens: Vec<&str> = raw.split_whitespace().flat_map(split_glued_command).collect();
    let mut out: Vec<String> = Vec::new();
    let mut skip = 0;
    // Set when a "2." became a colon: Whisper capitalised the next word only
    // because it read "2." as the end of a sentence.
    let mut lower_next = false;
    // An opening bracket or quote said aloud waits for the next word.
    let mut pending_open = String::new();
    let mut quote_open = false;
    let mut open_brackets = 0usize;

    for (i, token) in tokens.iter().enumerate() {
        if skip > 0 {
            skip -= 1;
            continue;
        }
        if let Some((text, used)) = dict.lookup_phrase(&tokens[i..]) {
            // A dictionary phrase wins over commands and fillers inside it, and
            // its spelling is kept exactly, like a one-word entry.
            lower_next = false;
            out.push(format!("{}{text}", std::mem::take(&mut pending_open)));
            skip = used - 1;
            continue;
        }

        let (core, punct) = split_trailing_punct(token);

        if core.is_empty() {
            // Stray punctuation token: attach to the previous word.
            if let Some(prev) = out.last_mut() {
                if !prev.ends_with(TRAILING_PUNCT) && !is_break(prev) {
                    prev.push_str(punct);
                }
            }
            continue;
        }

        if let Some((pair, used)) = paired_mark(&tokens, i, out.last(), quote_open, open_brackets) {
            skip = used - 1;
            match pair {
                Paired::Open(mark) => {
                    if !QUOTE_MARKS.contains(&mark) {
                        // "tejet, (két liter)": the pause before a bracket is not a comma.
                        if let Some(prev) = out.last_mut() {
                            let trimmed = prev.trim_end_matches(',').len();
                            prev.truncate(trimmed);
                        }
                        open_brackets += 1;
                    } else {
                        quote_open = true;
                    }
                    pending_open.push_str(mark);
                }
                Paired::Close(mark) => {
                    if QUOTE_MARKS.contains(&mark) {
                        quote_open = false;
                    } else {
                        open_brackets -= 1;
                    }
                    let tail = split_trailing_punct(tokens[i + used - 1]).1;
                    if let Some(prev) = out.last_mut() {
                        let trimmed = prev.trim_end_matches([',', ';', ':']).len();
                        prev.truncate(trimmed);
                        // A full stop the pause put before the mark goes after it
                        // ("(kész.)" → "(kész)."); an abbreviation keeps its own ("Kft.)").
                        let mut tail = tail.to_string();
                        if ends_sentence(prev) && prev.ends_with('.') {
                            prev.pop();
                            if !tail.starts_with(TRAILING_PUNCT) {
                                tail.insert(0, '.');
                            }
                        } else if prev.ends_with('.') {
                            tail = tail.trim_start_matches('.').to_string();
                        }
                        prev.push_str(mark);
                        prev.push_str(&tail);
                    }
                }
                Paired::Toggle(..) => unreachable!("paired_mark resolves toggles"),
            }
            continue;
        }

        if let Some((mark, used)) = spoken_mark(&tokens, i, out.last()) {
            lower_next = mark == ":" && tokens[i] == "2.";
            apply_mark(&mut out, mark);
            skip = used - 1;
            continue;
        }

        if is_filler(core) {
            // Keep sentence-ending punctuation the filler carried ("holnap öö." → "holnap.").
            let ending = punct.trim_start_matches(',');
            if !ending.is_empty() {
                if let Some(prev) = out.last_mut() {
                    let trimmed = prev.trim_end_matches(',').len();
                    prev.truncate(trimmed);
                    if !prev.ends_with(TRAILING_PUNCT) && !is_break(prev) {
                        prev.push_str(ending);
                    }
                }
            }
            continue;
        }

        let from_dict = dict.lookup_wrapped(core);
        let after_heard_colon = std::mem::take(&mut lower_next);
        let word = match &from_dict {
            Some(to) => to.clone(),
            None => fix_number_suffix(core)
                .or_else(|| fix_clock_dot(core, tokens.get(i + 1).copied()))
                .unwrap_or_else(|| core.to_string()),
        };

        // A stuttered function word ("a a", "the the") is a disfluency, unless
        // punctuation separates the two.
        if let Some(prev) = out.last() {
            if prev.to_lowercase() == word.to_lowercase() && is_stutter_word(&word) {
                out.pop();
            }
        }

        let after_sentence_end = out.last().is_some_and(|prev| ends_sentence(prev) || is_break(prev));
        // A dictionary spelling is kept exactly ("iPhone" stays "iPhone").
        let word = if after_sentence_end && from_dict.is_none() {
            capitalize_first(&word)
        } else if after_heard_colon && from_dict.is_none() {
            lowercase_first(&word)
        } else {
            word
        };
        let punct = if from_dict.is_some() { dedupe_stop(&word, punct) } else { punct };
        out.push(format!("{}{word}{punct}", std::mem::take(&mut pending_open)));
    }

    let mut text = String::new();
    for (i, word) in out.iter().enumerate() {
        if i > 0 && !is_break(word) && !is_break(&out[i - 1]) {
            text.push(' ');
        }
        text.push_str(word);
    }
    // A comma left dangling at the very end (from a dropped filler) is noise.
    while text.ends_with(',') {
        text.pop();
    }
    capitalize_first(&text)
}

/// A line or paragraph break produced by a spoken command ("új sor").
fn is_break(token: &str) -> bool {
    token.starts_with('\n')
}

/// A spoken command: its words (lower case, without the punctuation Whisper
/// may add) and what it becomes.
type Command<T> = (Vec<&'static str>, T);

/// The mark for each key of a pack's `[marks]` table.
const MARK_NAMES: &[(&str, &str)] = &[
    ("paragraph", "\n\n"),
    ("line", "\n"),
    ("question", "?"),
    ("exclamation", "!"),
    ("colon", ":"),
    ("semicolon", ";"),
    ("comma", ","),
    ("full_stop", "."),
];

/// Reads table `table` of every language pack into commands, longest first
/// so a longer command wins over a shorter one it starts with ("idézőjel
/// vége" over "idézőjel"). An unknown key is a mistake in a compiled-in pack.
fn load_commands<T: Copy>(table: &str, names: &[(&str, T)]) -> Vec<Command<T>> {
    let mut out = Vec::new();
    for (name, phrases) in lang::table(table) {
        let value = names
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("language pack: unknown [{table}] key {name}"))
            .1;
        for phrase in phrases {
            out.push((phrase.split(' ').collect(), value));
        }
    }
    out.sort_by_key(|(words, _): &Command<T>| std::cmp::Reverse(words.len()));
    out
}

/// Spoken punctuation and layout commands from the `[marks]` tables.
fn spoken_marks() -> &'static [Command<&'static str>] {
    static MARKS: OnceLock<Vec<Command<&'static str>>> = OnceLock::new();
    MARKS.get_or_init(|| load_commands("marks", MARK_NAMES))
}

/// Whisper sometimes glues a command to the sentence before it
/// ("jövök.új bekezdés"): split such a token after its full stop when the rest
/// starts a command. Other dotted words ("vatson.ai", "e.g.") stay whole.
fn split_glued_command(token: &str) -> Vec<&str> {
    if let Some(dot) = token.find('.') {
        let (head, rest) = token.split_at(dot + 1);
        let rest_core = split_trailing_punct(rest).0.to_lowercase();
        let starts_command = spoken_marks().iter().any(|(words, _)| words[0] == rest_core);
        if head.len() > 1 && starts_command {
            return vec![head, rest];
        }
    }
    split_glued_comma(token)
}

/// The misspelt comma command Whisper glues to the words around it.
const GLUED_COMMA: &str = "vesző";

/// Verbal prefixes that make "-vesző" a real word ("elvesző", "kiveszőben").
const VERB_PREFIXES: &[&str] = &[
    "el", "ki", "be", "le", "fel", "föl", "meg", "át", "rá", "szét", "össze", "vissza", "oda",
    "ide", "körül", "keresztül", "hozzá", "agyon",
];

/// Whisper sometimes writes the spoken comma "vessző" glued to its neighbours
/// ("az árvesző", "almaveszőkörteveszőszilva"): split such a token into words
/// and commands. Only the misspelt "vesző" counts (real compounds keep the
/// double s: "tizedesvessző"), never after a verbal prefix ("elvesző") or a
/// semicolon spelling ("pontosvesző"), and never before "-d" ("vesződik") or
/// a suffix ("kiveszőben").
fn split_glued_comma(token: &str) -> Vec<&str> {
    let (core, _) = split_trailing_punct(token);
    let lower = core.to_lowercase();
    // Lowercasing can change byte lengths only outside Hungarian letters; bail out then.
    if lower.len() != core.len() || match_mark(&[token], 0).is_some() {
        return vec![token];
    }
    let mut parts = Vec::new();
    let mut start = 0;
    let mut search = 0;
    while let Some(off) = lower[search..].find(GLUED_COMMA) {
        let at = search + off;
        let end = at + GLUED_COMMA.len();
        let head = &lower[start..at];
        let tail = &lower[end..];
        let head_ok = head.chars().count() >= 2
            && head.chars().all(char::is_alphabetic)
            && !VERB_PREFIXES.contains(&head)
            && !head.ends_with("pontos")
            && !head.ends_with("pontoz");
        let tail_ok = tail.is_empty()
            || (tail.chars().count() >= 3
                && !tail.starts_with('d')
                && !["ben", "re", "ként"].iter().any(|s| tail.starts_with(s)));
        if !head_ok || !tail_ok {
            search = end;
            continue;
        }
        parts.push(&token[start..at]);
        start = at;
        search = end;
        if tail.is_empty() {
            break;
        }
        parts.push(&token[at..end]);
        start = end;
    }
    if parts.is_empty() {
        return vec![token];
    }
    parts.push(&token[start..]);
    parts
}

/// Words that make the next word a noun, not a command ("a vessző"): the
/// packs' `noun_markers`. Commands that are also everyday words ("pont úgy",
/// "the period") are the packs' `ambiguous_marks`: they count only at the very
/// end of the dictation or where Whisper already put punctuation after them,
/// i.e. where the speaker paused, or right before another command ("pont, új
/// bekezdés").
fn is_noun_marker(word: &str) -> bool {
    lang::contains(lang::NOUN_MARKERS, word)
}

/// If a spoken command starts at `tokens[i]`, returns its mark and how many
/// tokens it spans. A command needs a real word before it to attach to; only
/// a break may follow another break ("new line new line" is a blank line).
fn spoken_mark(tokens: &[&str], i: usize, prev: Option<&String>) -> Option<(&'static str, usize)> {
    let prev = prev?;
    if is_break(prev) {
        return match_mark(tokens, i).filter(|(mark, _)| is_break(mark));
    }
    let prev_core = split_trailing_punct(prev).0.to_lowercase();
    // After a number the word counts something ("kaptam 70 pont").
    let counted = prev_core.chars().next().is_some_and(|c| c.is_ascii_digit());
    if counted || is_noun_marker(&prev_core) {
        return None;
    }
    if is_heard_colon(tokens, i, &prev_core) {
        return Some((":", 1));
    }
    let (mark, len) = match_mark(tokens, i)?;
    let last = i + len - 1;
    if len == 1 && lang::contains(lang::AMBIGUOUS_MARKS, &split_trailing_punct(tokens[i]).0.to_lowercase()) {
        let paused = !split_trailing_punct(tokens[last]).1.is_empty();
        if last + 1 < tokens.len() && !paused && match_mark(tokens, last + 1).is_none() {
            return None;
        }
    }
    Some((mark, len))
}

/// Whisper often hears "kettőspont" as "kettő pont" and writes "2." ("A lista
/// 2. Alma, körte"). Read "2." as a colon only between a plain word and a
/// capitalised one, and never in a numbered list (a "1." earlier) or after an
/// article ("a 2. Világháború", rejected before this is called).
fn is_heard_colon(tokens: &[&str], i: usize, prev_core: &str) -> bool {
    tokens[i] == "2."
        && prev_core.chars().all(char::is_alphabetic)
        && !tokens[..i].contains(&"1.")
        && tokens.get(i + 1).and_then(|t| t.chars().next()).is_some_and(char::is_uppercase)
}

/// The spoken command starting at `tokens[i]`, if any, and how many tokens it spans.
fn match_mark(tokens: &[&str], i: usize) -> Option<(&'static str, usize)> {
    'marks: for (words, mark) in spoken_marks() {
        if i + words.len() > tokens.len() {
            continue;
        }
        for (k, want) in words.iter().enumerate() {
            let (core, punct) = split_trailing_punct(tokens[i + k]);
            // Only the last word of a command may carry punctuation.
            if core.to_lowercase() != *want || (k + 1 < words.len() && !punct.is_empty()) {
                continue 'marks;
            }
        }
        return Some((mark, words.len()));
    }
    None
}

/// A bracket or quote said aloud. An opening mark joins the next word, a
/// closing mark the previous one; `Toggle` opens or closes a quote.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Paired {
    Open(&'static str),
    Close(&'static str),
    Toggle(&'static str, &'static str),
}

/// Quote characters: Hungarian „…”, English straight quotes (safe in code and plain text).
const QUOTE_MARKS: &[&str] = &["„", "”", "\""];

/// The bracket or quote for each key of a pack's `[paired]` table.
const PAIRED_NAMES: &[(&str, Paired)] = &[
    ("open_paren", Paired::Open("(")),
    ("close_paren", Paired::Close(")")),
    ("open_low_quote", Paired::Open("„")),
    ("close_low_quote", Paired::Close("”")),
    ("low_quote", Paired::Toggle("„", "”")),
    ("open_straight_quote", Paired::Open("\"")),
    ("close_straight_quote", Paired::Close("\"")),
    ("straight_quote", Paired::Toggle("\"", "\"")),
];

/// Spoken brackets and quotes from the `[paired]` tables. Misspellings are
/// what Whisper writes for TTS speech ("mitó zárójel", "open parin").
fn paired_marks() -> &'static [Command<Paired>] {
    static PAIRED: OnceLock<Vec<Command<Paired>>> = OnceLock::new();
    PAIRED.get_or_init(|| load_commands("paired", PAIRED_NAMES))
}

fn match_paired(tokens: &[&str], i: usize) -> Option<(Paired, usize)> {
    'marks: for (words, pair) in paired_marks() {
        if i + words.len() > tokens.len() {
            continue;
        }
        for (k, want) in words.iter().enumerate() {
            let (core, punct) = split_trailing_punct(tokens[i + k]);
            if core.to_lowercase() != *want || (k + 1 < words.len() && !punct.is_empty()) {
                continue 'marks;
            }
        }
        return Some((*pair, words.len()));
    }
    None
}

/// The spoken bracket or quote starting at `tokens[i]`, resolved to an opening
/// or closing mark, and how many tokens it spans. Like other commands it is a
/// noun after an article ("az idézőjel", "a quote") or a number. A lone
/// "idézőjel" / "quote" opens a quote only when a closing one follows
/// ("send me a quote" stays a word); while a quote is open it closes it. A
/// closing mark counts only when its opening one was said ("demo unquote"
/// with the "quote" misheard stays as heard).
fn paired_mark(
    tokens: &[&str],
    i: usize,
    prev: Option<&String>,
    quote_open: bool,
    open_brackets: usize,
) -> Option<(Paired, usize)> {
    if let Some(prev) = prev {
        let prev_core = split_trailing_punct(prev).0.to_lowercase();
        let counted = prev_core.chars().next().is_some_and(|c| c.is_ascii_digit());
        if counted || is_noun_marker(&prev_core) {
            return None;
        }
    }
    let (pair, used) = match_paired(tokens, i)?;
    let (open, close) = match pair {
        Paired::Open(_) => return Some((pair, used)),
        Paired::Close(mark) => {
            let is_open = if QUOTE_MARKS.contains(&mark) { quote_open } else { open_brackets > 0 };
            return (is_open && prev.is_some()).then_some((pair, used));
        }
        Paired::Toggle(open, close) => (open, close),
    };
    if quote_open {
        return prev.is_some().then_some((Paired::Close(close), used));
    }
    let closes_later = (i + used..tokens.len()).any(|j| {
        matches!(match_paired(tokens, j), Some((Paired::Toggle(..), _)))
            || matches!(match_paired(tokens, j), Some((Paired::Close(c), _)) if c == close)
    });
    closes_later.then_some((Paired::Open(open), used))
}

/// Attaches a spoken mark to the text so far. A mark replaces the commas,
/// colons or a single full stop Whisper put before it ("jössz, kérdőjel" →
/// "jössz?"); a break also drops the comma before it.
fn apply_mark(out: &mut Vec<String>, mark: &str) {
    let Some(prev) = out.last_mut() else { return };
    let trimmed = prev.trim_end_matches([',', ';', ':']).len();
    prev.truncate(trimmed);
    if is_break(mark) {
        out.push(mark.to_string());
        return;
    }
    if prev.ends_with(['?', '!', '…']) || prev.ends_with("..") {
        return;
    }
    if prev.ends_with('.') && mark != "." {
        // Keep the full stop of an abbreviation or ordinal ("Kft.", "13.").
        if ends_sentence(prev) {
            prev.pop();
        } else if mark == "," {
            return;
        }
    } else if prev.ends_with('.') {
        return;
    }
    prev.push_str(mark);
}

/// Short function words whose immediate repetition is always a stutter
/// (`stutter_words` in the language packs).
fn is_stutter_word(word: &str) -> bool {
    lang::contains(lang::STUTTER_WORDS, &word.to_lowercase())
}

/// True when `token` (a finished output word) closes a sentence, so the next word
/// starts with a capital. "?" and "!" always do. A full stop does unless it ends
/// a number ("a 13. helyen", "2024. március") or a known abbreviation.
fn ends_sentence(token: &str) -> bool {
    if token.ends_with('?') || token.ends_with('!') {
        return true;
    }
    if !token.ends_with('.') || token.ends_with("..") {
        return false;
    }
    // A dictionary spelling can hold several words ("Clausis Kft."): judge the last.
    let core = token
        .trim_end_matches(TRAILING_PUNCT)
        .rsplit(' ')
        .next()
        .unwrap_or_default();
    if core.is_empty() || core.ends_with(|c: char| c.is_ascii_digit()) {
        return false;
    }
    // A dotted abbreviation ("p.m.", "U.S.", "e.g.") may or may not end the
    // sentence; the engine already capitalises a real sentence start, so keep
    // the next word as written.
    if core.contains('.') {
        return false;
    }
    !lang::contains(lang::ABBREVIATIONS, &core.to_lowercase()) && !lang::contains(lang::LOWERCASE_ABBREVIATIONS, core)
}

/// Hungarian instrumental suffix ("-val/-vel") written after a number or a percent
/// sign. The suffix assimilates to the last sound of the spoken number, so
/// "15%-al" must read "15%-kal" (százalékkal) and "5-el" must read "5-tel" (öttel).
/// Returns the corrected token, or None when the token is not such a case.
fn fix_number_suffix(core: &str) -> Option<String> {
    let (stem, suffix) = core.rsplit_once('-')?;
    if !matches!(suffix, "al" | "el" | "val" | "vel") {
        return None;
    }
    let correct = if let Some(digits) = stem.strip_suffix('%') {
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.') {
            return None;
        }
        "kal"
    } else {
        if stem.is_empty() || !stem.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        instrumental_after(stem.parse::<u64>().ok()?)
    };
    (suffix != correct).then(|| format!("{stem}-{correct}"))
}

/// Whisper writes an English clock time with a dot ("3.30 p.m."); US and UK
/// usage both read "3:30 p.m.". Only rewritten right before "a.m."/"p.m." (or
/// "am"/"pm"), where the token cannot be a decimal or a Hungarian date.
fn fix_clock_dot(core: &str, next: Option<&str>) -> Option<String> {
    let next = next?.trim_end_matches(TRAILING_PUNCT).to_lowercase();
    if !matches!(next.as_str(), "a.m" | "p.m" | "am" | "pm") {
        return None;
    }
    let (hours, minutes) = core.split_once('.')?;
    let hour: u32 = hours.parse().ok().filter(|_| (1..=2).contains(&hours.len()))?;
    let minute: u32 = minutes.parse().ok().filter(|_| minutes.len() == 2)?;
    (hour <= 12 && minute < 60).then(|| format!("{hours}:{minutes}"))
}

/// The instrumental suffix for an integer, from the last number word it is read with.
fn instrumental_after(n: u64) -> &'static str {
    const UNITS: [&str; 10] = ["", "gyel", "vel", "mal", "gyel", "tel", "tal", "tel", "cal", "cel"];
    const TENS: [&str; 10] = ["", "zel", "szal", "cal", "nel", "nel", "nal", "nel", "nal", "nel"];
    if n == 0 {
        "val" // nulla
    } else if !n.is_multiple_of(10) {
        UNITS[(n % 10) as usize]
    } else if !n.is_multiple_of(100) {
        TENS[((n / 10) % 10) as usize]
    } else if !n.is_multiple_of(1000) {
        "zal" // száz
    } else if !n.is_multiple_of(1_000_000) {
        "rel" // ezer
    } else if !n.is_multiple_of(1_000_000_000) {
        "val" // millió
    } else {
        "dal" // milliárd
    }
}

/// Lowercases the first letter unless the word looks like an acronym ("EU", "HR").
fn lowercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(_), Some(second)) if second.is_uppercase() => s.to_string(),
        (Some(first), _) => first.to_lowercase().chain(s.chars().skip(1)).collect(),
        (None, _) => String::new(),
    }
}

fn capitalize_first(s: &str) -> String {
    // "(kedden…" / "„szia…": the capital goes after an opening bracket or quote.
    let opening = s.len() - s.trim_start_matches(['(', '„', '"']).len();
    if opening > 0 {
        return format!("{}{}", &s[..opening], capitalize_first(&s[opening..]));
    }
    // A word that already has a capital after its first letter has its own
    // shape ("iOS", "macOS", "iPhone", "eBay"); a capital there would break it.
    let first_word = s.split_whitespace().next().unwrap_or("");
    if first_word.chars().skip(1).any(char::is_uppercase) {
        return s.to_string();
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_commands_load_longest_first_without_clashes() {
        let marks = spoken_marks();
        let paired = paired_marks();
        assert_eq!((marks.len(), paired.len()), (28, 25));
        assert!(marks.windows(2).all(|w| w[0].0.len() >= w[1].0.len()));
        assert!(paired.windows(2).all(|w| w[0].0.len() >= w[1].0.len()));
        let mut seen = std::collections::HashSet::new();
        for words in marks.iter().map(|c| &c.0).chain(paired.iter().map(|c| &c.0)) {
            assert!(words.iter().all(|w| !w.is_empty() && w.to_lowercase() == *w), "{words:?}");
            assert!(seen.insert(words.clone()), "said twice: {words:?}");
        }
        assert_eq!(match_mark(&["új", "bekezdés"], 0), Some(("\n\n", 2)));
        assert_eq!(match_paired(&["idézőjel", "vége"], 0), Some((Paired::Close("”"), 2)));
        assert_eq!(match_paired(&["quote"], 0), Some((Paired::Toggle("\"", "\""), 1)));
    }

    fn c(s: &str) -> String {
        clean(s, &Dictionary::new())
    }

    #[test]
    fn removes_hungarian_hesitation() {
        assert_eq!(c("öö holnap találkozunk"), "Holnap találkozunk");
        assert_eq!(c("Szóval, öö, holnap jövök."), "Szóval, holnap jövök.");
        assert_eq!(c("megcsinálom ööö."), "Megcsinálom.");
    }

    #[test]
    fn keeps_camel_case_words_at_sentence_start() {
        assert_eq!(c("iOS frissítés kell"), "iOS frissítés kell");
        assert_eq!(c("macOS is fine"), "macOS is fine");
        assert_eq!(c("eBay-en vettem"), "eBay-en vettem");
        assert_eq!(c("Mondd meg neki. iPhone kell."), "Mondd meg neki. iPhone kell.");
        // Ordinary words and all-lowercase brands are still capitalised.
        assert_eq!(c("ok. iphone kell"), "Ok. Iphone kell");
        assert_eq!(c("ő is jön"), "Ő is jön");
    }

    #[test]
    fn keeps_real_discourse_words() {
        assert_eq!(c("hát szóval tehát ugye ez jó"), "Hát szóval tehát ugye ez jó");
        // "ő" is a pronoun (he/she), never a filler.
        assert_eq!(c("ő is jön"), "Ő is jön");
    }

    #[test]
    fn removes_english_fillers() {
        assert_eq!(c("um so I uh pushed the branch"), "So I pushed the branch");
    }

    #[test]
    fn collapses_whitespace_and_stray_punctuation() {
        assert_eq!(c("  szia   , mi   újság ?"), "Szia, mi újság?");
    }

    #[test]
    fn dictionary_fixes_spelling() {
        let mut d = Dictionary::new();
        d.add("klózisz", "Clausis");
        d.add("vatszon", "Vatson");
        assert_eq!(clean("a klózisz és a vatszon, öö, kész.", &d), "A Clausis és a Vatson, kész.");
        assert_eq!(d.prompt(), format!("{HU_PROMPT_LABEL} Clausis, Vatson."));
        assert_eq!(Dictionary::new().prompt(), "");
        let mut inc = Dictionary::new();
        inc.add("acme inc", "Acme Inc.");
        assert_eq!(inc.prompt(), format!("{HU_PROMPT_LABEL} Acme Inc."));
        d.add("ájfón", "iPhone");
        assert_eq!(clean("kész? ájfón jön", &d), "Kész? iPhone jön");
    }

    #[test]
    fn dictionary_matches_wrapped_and_suffixed_words() {
        let mut d = Dictionary::new();
        d.add("kubernetes", "Kubernetes");
        d.add("pusmogo", "Pusmogo");
        d.add("wi-fi", "Wi-Fi");
        d.add("acme inc", "Acme Inc.");
        d.add("acme", "Acme Inc.");
        // Quotes and brackets around the word, with punctuation inside or out.
        assert_eq!(clean("a (kubernetes) fut", &d), "A (Kubernetes) fut");
        assert_eq!(clean("a \"pusmogo\", ugye", &d), "A \"Pusmogo\", ugye");
        assert_eq!(clean("ez a „pusmogo”.", &d), "Ez a „Pusmogo”.");
        assert_eq!(clean("lásd [kubernetes.]", &d), "Lásd [Kubernetes.]");
        assert_eq!(clean("az (acme.)", &d), "Az (Acme Inc.)");
        // Hungarian suffix after a hyphen.
        assert_eq!(clean("letöltöttem a pusmogo-t.", &d), "Letöltöttem a Pusmogo-t.");
        assert_eq!(clean("a kubernetes-ben, ugye", &d), "A Kubernetes-ben, ugye");
        assert_eq!(clean("a (pusmogo-val)", &d), "A (Pusmogo-val)");
        // A whole-word entry wins; unknown stems and non-suffixes stay.
        assert_eq!(clean("a wi-fi megy", &d), "A Wi-Fi megy");
        assert_eq!(clean("a self-service és a pusmogo-API", &d), "A self-service és a pusmogo-API");
        assert_eq!(clean("a (valami) és \"más\"", &d), "A (valami) és \"más\"");
        // A glued "..." or a hashtag in front stays in front.
        assert_eq!(clean("szóval ...kubernetes megy", &d), "Szóval ...Kubernetes megy");
        assert_eq!(clean("szóval …pusmogo-t", &d), "Szóval …Pusmogo-t");
        assert_eq!(clean("a #pusmogo és a #acme inc", &d), "A #Pusmogo és a #Acme Inc.");
        // Words joined by a slash; links and paths stay as written.
        assert_eq!(clean("a kubernetes/pusmogo-t", &d), "A Kubernetes/Pusmogo-t");
        assert_eq!(clean("(valami/kubernetes).", &d), "(valami/Kubernetes).");
        assert_eq!(clean("lásd example.com/pusmogo és https://x/pusmogo", &d), "Lásd example.com/pusmogo és https://x/pusmogo");
        assert_eq!(clean("a/b és /", &d), "A/b és /");
    }

    #[test]
    fn dictionary_prompt_fits_token_budget() {
        let mut d = Dictionary::new();
        for w in ["Alfa", "Béta", "Gamma", "Delta"] {
            d.add(w, w);
        }
        let chars = |s: &str| Some(s.chars().count());
        // Fits: unchanged.
        assert_eq!(d.prompt_fitting(100, chars), d.prompt());
        // Too long: the oldest spellings go, the label stays.
        let p = d.prompt_fitting(HU_PROMPT_LABEL.chars().count() + 14, chars);
        assert_eq!(p, format!("{HU_PROMPT_LABEL} Gamma, Delta."));
        // No tokenizer, or nothing fits: never loops forever.
        assert_eq!(d.prompt_fitting(5, |_| None), d.prompt());
        // Context hints go after the dictionary, without duplicates, and an
        // empty list changes nothing.
        assert_eq!(d.prompt_fitting_with(&[], Language::Hungarian, 100, chars), d.prompt_fitting(100, chars));
        assert_eq!(d.prompt_fitting_with(&[], Language::English, 100, chars), "Common words: Alfa, Béta, Gamma, Delta.");
        let extra = vec!["Bence".to_string(), d.spellings()[0].to_lowercase()];
        let with = d.prompt_fitting_with(&extra, Language::Hungarian, 1000, chars);
        assert!(with.ends_with(", Bence."), "{with}");
        assert_eq!(with.matches(", ").count(), d.spellings().len());
        assert_eq!(d.prompt_fitting(5, chars), "");
        assert_eq!(d.prompt_left_out(5, |_| None), None);
        assert_eq!(d.prompt_left_out(100, chars), Some(0));
        assert_eq!(d.prompt_left_out(5, chars), Some(d.spellings().len()));
        // Replacements still apply to entries left out of the prompt.
        assert_eq!(clean("alfa", &d), "Alfa");
    }

    #[test]
    fn fixes_instrumental_after_numbers() {
        assert_eq!(c("A költségvetés 15%-al nőtt."), "A költségvetés 15%-kal nőtt.");
        assert_eq!(c("a forgalom 30%-al emelkedett"), "A forgalom 30%-kal emelkedett");
        assert_eq!(c("2,5%-al több"), "2,5%-kal több");
        assert_eq!(c("5-el, 3-al, 8-al, 10-el, 20-al, 100-al, 1000-el"),
            "5-tel, 3-mal, 8-cal, 10-zel, 20-szal, 100-zal, 1000-rel");
        assert_eq!(c("1-el 4-el 7-el 9-el 40-el 60-al 2000000-el"),
            "1-gyel 4-gyel 7-tel 9-cel 40-nel 60-nal 2000000-val");
        // Already correct, other suffixes and words stay untouched.
        assert_eq!(c("15%-kal 2-vel 8-kor 3-as busz e-mail"), "15%-kal 2-vel 8-kor 3-as busz e-mail");
    }

    #[test]
    fn removes_stuttered_function_words() {
        assert_eq!(c("a a projekt kész"), "A projekt kész");
        assert_eq!(c("I pushed the the branch"), "I pushed the branch");
        assert_eq!(c("és és hogy hogy"), "És hogy");
        // Legitimate repetitions and punctuated ones stay.
        assert_eq!(c("nagyon nagyon jó"), "Nagyon nagyon jó");
        assert_eq!(c("I know that that works"), "I know that that works");
        assert_eq!(c("a, a"), "A, a");
    }

    #[test]
    fn capitalises_after_sentence_end() {
        assert_eq!(c("kész vagy? holnap jövök! szia. jó"), "Kész vagy? Holnap jövök! Szia. Jó");
        // Ordinals and dates keep their lower-case continuation (real bench output).
        assert_eq!(c("a 4., 5., illetve 6. helyen végeztek"), "A 4., 5., illetve 6. helyen végeztek");
        assert_eq!(c("2024. március 5. kedd"), "2024. március 5. kedd");
        // Abbreviations and ellipses do not end a sentence.
        assert_eq!(c("pl. alma, stb. és Mr. smith"), "Pl. alma, stb. és Mr. smith");
        assert_eq!(c("hát... nem tudom"), "Hát... nem tudom");
        // Everyday Hungarian abbreviations keep the next word lower case.
        assert_eq!(c("kérek 5 db. almát"), "Kérek 5 db. almát");
        assert_eq!(c("a tel. számon hívj"), "A tel. számon hívj");
        assert_eq!(c("max. öt percet várj, min. két főre"), "Max. öt percet várj, min. két főre");
        assert_eq!(c("ld. a mellékletet"), "Ld. a mellékletet");
        // Dotted abbreviations keep the next word as the engine wrote it.
        assert_eq!(c("meet at 5 p.m. today"), "Meet at 5 p.m. today");
        assert_eq!(c("the U.S. economy grew"), "The U.S. economy grew");
        assert_eq!(c("a U.S.A. trip"), "A U.S.A. trip");
        assert_eq!(c("see you at 5 p.m. Then we eat"), "See you at 5 p.m. Then we eat");
        // A capitalised Max or Min is a name and can end a sentence.
        assert_eq!(c("I called Max. he answered"), "I called Max. He answered");
        // Company forms, typed or from a multi-word dictionary spelling.
        assert_eq!(c("a Bosch Kft. csapata és az OTP Nyrt. vezetője"), "A Bosch Kft. csapata és az OTP Nyrt. vezetője");
        let mut dict = Dictionary::new();
        dict.add("klózisz", "Clausis Kft.");
        assert_eq!(clean("a klózisz csapat", &dict), "A Clausis Kft. csapat");
        assert_eq!(clean("ez a klózisz.", &dict), "Ez a Clausis Kft.");
    }

    #[test]
    fn dictionary_matches_phrases() {
        let mut d = Dictionary::new();
        d.add("open ai", "OpenAI");
        d.add("vs code", "VS Code");
        d.add("code", "kód");
        d.add("acme  inc", "Acme Inc.");
        d.add("new york times", "The New York Times");
        d.add("new york", "New York");
        // Two words, mid-sentence, at the start, and before a comma.
        assert_eq!(clean("az open ai modellje.", &d), "Az OpenAI modellje.");
        assert_eq!(clean("Open AI, a cég.", &d), "OpenAI, a cég.");
        assert_eq!(clean("open ai", &d), "OpenAI");
        // The longest phrase wins over a shorter one and over a one-word entry.
        assert_eq!(clean("I read new york times today", &d), "I read The New York Times today");
        assert_eq!(clean("I flew to new york.", &d), "I flew to New York.");
        assert_eq!(clean("open vs code now", &d), "Open VS Code now");
        assert_eq!(clean("the code works", &d), "The kód works");
        // A full stop the spelling already has is not doubled.
        assert_eq!(clean("we bought acme inc.", &d), "We bought Acme Inc.");
        // Punctuation inside the phrase breaks it; a partial match is no match.
        assert_eq!(clean("open, ai said", &d), "Open, ai said");
        assert_eq!(clean("open the door", &d), "Open the door");
        // After a sentence end the spelling stays exact.
        assert_eq!(clean("Kész. open ai jön", &d), "Kész. OpenAI jön");
    }

    #[test]
    fn dictionary_matches_wrapped_and_suffixed_phrases() {
        let mut d = Dictionary::new();
        d.add("open ai", "OpenAI");
        d.add("acme inc", "Acme Inc.");
        d.add("new york times", "The New York Times");
        // Brackets and quotes around the phrase are kept.
        assert_eq!(clean("a cég (open ai) modellje", &d), "A cég (OpenAI) modellje");
        assert_eq!(clean("ezt írta a „new york times”.", &d), "Ezt írta a „The New York Times”.");
        assert_eq!(clean("(open ai).", &d), "(OpenAI).");
        // A Hungarian suffix after a hyphen stays on the spelling.
        assert_eq!(clean("az open ai-t kérdeztem.", &d), "Az OpenAI-t kérdeztem.");
        assert_eq!(clean("(open ai-val) beszéltem", &d), "(OpenAI-val) beszéltem");
        // A spelling's own full stop is not doubled inside a bracket.
        assert_eq!(clean("a cég (acme inc.) szerint", &d), "A cég (Acme Inc.) szerint");
        // A wrap inside the phrase or an uppercase tail still breaks it.
        assert_eq!(clean("open (ai) said", &d), "Open (ai) said");
        assert_eq!(clean("open) ai said", &d), "Open) ai said");
        assert_eq!(clean("open ai-API here", &d), "Open ai-API here");
    }

    #[test]
    fn dictionary_keeps_english_possessive() {
        let mut d = Dictionary::new();
        d.add("open ai", "OpenAI");
        d.add("vs code", "VS Code");
        d.add("gpt", "GPT");
        d.add("pusmogo", "Pusmogo");
        assert_eq!(clean("I like open ai's models", &d), "I like OpenAI's models");
        assert_eq!(clean("open ai’s model", &d), "OpenAI’s model");
        assert_eq!(clean("the vs code's settings.", &d), "The VS Code's settings.");
        assert_eq!(clean("gpt's answer", &d), "GPT's answer");
        assert_eq!(clean("(pusmogo’s) icon", &d), "(Pusmogo’s) icon");
        // Other tails and a bare apostrophe-s stay as they are.
        assert_eq!(clean("gpt'll do", &d), "Gpt'll do");
        assert_eq!(clean("it's 's fine", &d), "It's 's fine");
    }

    #[test]
    fn dictionary_keeps_version_tail() {
        let mut d = Dictionary::new();
        d.add("gpt", "GPT");
        d.add("open ai", "OpenAI");
        d.add("covid-19", "COVID-19");
        d.add("covid", "Covid");
        assert_eq!(clean("a gpt-4 modell", &d), "A GPT-4 modell");
        assert_eq!(clean("try gpt-4o and gpt-3.5.", &d), "Try GPT-4o and GPT-3.5.");
        assert_eq!(clean("a (gpt-4) és a gpt-4-et", &d), "A (GPT-4) és a GPT-4-et");
        assert_eq!(clean("az open ai-5 jön", &d), "Az OpenAI-5 jön");
        assert_eq!(clean("gpt-4's answer and gpt-4o’s", &d), "GPT-4's answer and GPT-4o’s");
        // A whole-word entry still wins; a tail without a leading digit stays.
        assert_eq!(clean("a covid-19 után", &d), "A COVID-19 után");
        assert_eq!(clean("a gpt-API és gpt- és -4", &d), "A gpt-API és gpt- és -4");
    }

    #[test]
    fn spoken_punctuation_hungarian() {
        assert_eq!(c("holnap jövök pont"), "Holnap jövök.");
        assert_eq!(c("holnap jövök pont. utána hívlak"), "Holnap jövök. Utána hívlak");
        assert_eq!(c("szia vessző mi újság kérdőjel"), "Szia, mi újság?");
        assert_eq!(c("Jössz holnap? Kérdőjel."), "Jössz holnap?");
        assert_eq!(c("ez nagyszerű felkiáltójel"), "Ez nagyszerű!");
        assert_eq!(c("a lista kettőspont alma vessző körte"), "A lista: alma, körte");
        assert_eq!(c("kész pontosvessző mehetünk"), "Kész; mehetünk");
        assert_eq!(c("Szia, új sor. Holnap jövök."), "Szia\nHolnap jövök.");
        assert_eq!(c("első pont új bekezdés második"), "Első.\n\nMásodik");
        assert_eq!(c("első újsor második"), "Első\nMásodik");
        // Real model output from a TTS clip: glued command, misspelt "vessző".
        assert_eq!(c("Szia vesző holnap jövök.új bekezdés hozol kávét?"), "Szia, holnap jövök.\n\nHozol kávét?");
        assert_eq!(c("lásd vatson.ai és pl.alma"), "Lásd vatson.ai és pl.alma");
        // Real model output from TTS clips (docs/bench/spoken-01.md).
        assert_eq!(c("Ez nagyszerű felkiáltó jel."), "Ez nagyszerű!");
        assert_eq!(c("Fontos ketőspont ne felejtsd el a kulcsot."), "Fontos: ne felejtsd el a kulcsot.");
        assert_eq!(c("Elküldtem a jelentést pontozvesző a többit holnap."), "Elküldtem a jelentést; a többit holnap.");
        assert_eq!(c("Elküldtem a jelentést, pontosvesző a többit holnap."), "Elküldtem a jelentést; a többit holnap.");
        // Whisper's capital after a heard "2." is dropped; acronyms keep theirs.
        assert_eq!(c("A lista 2. Alma, körte, szilva."), "A lista: alma, körte, szilva.");
        assert_eq!(c("Fontos 2. Ne felejtsd el a kulcsot."), "Fontos: ne felejtsd el a kulcsot.");
        assert_eq!(c("Résztvevők 2. EU és HR."), "Résztvevők: EU és HR.");
        assert_eq!(c("Ráérsz kérdő jel"), "Ráérsz?");
    }

    #[test]
    fn spoken_punctuation_english() {
        assert_eq!(c("see you tomorrow period"), "See you tomorrow.");
        assert_eq!(c("hi comma are you there question mark"), "Hi, are you there?");
        assert_eq!(c("great exclamation point new paragraph thanks full stop"), "Great!\n\nThanks.");
        assert_eq!(c("first new line second"), "First\nSecond");
    }

    #[test]
    fn spoken_brackets_and_quotes_hungarian() {
        // Whisper's spellings from TTS runs, commas from the pauses included.
        assert_eq!(
            c("A megbeszélés mitó zárójel kedden záró zárójel elmarad."),
            "A megbeszélés (kedden) elmarad."
        );
        assert_eq!(
            c("Vegyél tejet, nyító zárójel, két liter zárózárójel és kenyeret."),
            "Vegyél tejet (két liter) és kenyeret."
        );
        assert_eq!(c("Ez fontos, nyitó zárójel, nagyon záró zárójel."), "Ez fontos (nagyon).");
        assert_eq!(
            c("Azt mondta, idézőjel holnap jövök idézőjel, és elment."),
            "Azt mondta, „holnap jövök”, és elment."
        );
        assert_eq!(c("A cím nyitó idézőjel Pusmogo idézőjel vége lesz."), "A cím „Pusmogo” lesz.");
        assert_eq!(c("Idézőjel szia idézőjel"), "„Szia”");
        // A sentence ending inside a bracket still capitalises the next one.
        assert_eq!(c("ok nyitó zárójel kész. záró zárójel aztán megyünk"), "Ok (kész). Aztán megyünk");
    }

    #[test]
    fn spoken_brackets_and_quotes_english() {
        assert_eq!(
            c("The meeting open bracket on Tuesday, close bracket is canceled."),
            "The meeting (on Tuesday) is canceled."
        );
        assert_eq!(
            c("She said, quote, I will be late, end quote, and left."),
            "She said, \"I will be late\", and left."
        );
        assert_eq!(c("Buy milk, open parin, two liters, close parin, and bread."), "Buy milk (two liters), and bread.");
        assert_eq!(c("the title is quote demo unquote"), "The title is \"demo\"");
    }

    #[test]
    fn bracket_and_quote_words_stay_words() {
        // Inflected, after an article, or a lone "quote" with nothing closing it.
        assert_eq!(c("Ezt írd zárójelbe, a zárójelben lévő rész fontos."), "Ezt írd zárójelbe, a zárójelben lévő rész fontos.");
        assert_eq!(c("Hiányzik a záró zárójel a kódból."), "Hiányzik a záró zárójel a kódból.");
        assert_eq!(c("Tedd ki az idézőjel jelet."), "Tedd ki az idézőjel jelet.");
        assert_eq!(c("Send me a quote for the job."), "Send me a quote for the job.");
        assert_eq!(c("Quote of the day is short."), "Quote of the day is short.");
        assert_eq!(c("Idézőjel nélkül írd."), "Idézőjel nélkül írd.");
        assert_eq!(c("The open bracket is missing."), "The open bracket is missing.");
        // A closing command with nothing open (the opening one misheard).
        assert_eq!(c("The title is Quotepus Mogo Demo Unquote."), "The title is Quotepus Mogo Demo Unquote.");
        assert_eq!(c("Kész záró zárójel."), "Kész záró zárójel.");
    }

    #[test]
    fn repeated_break_commands_stack() {
        assert_eq!(c("Hello new line new line Thanks"), "Hello\n\nThanks");
        assert_eq!(c("Szia új sor új sor hogy vagy"), "Szia\n\nHogy vagy");
        assert_eq!(c("Szia, új sor. Új sor. Holnap jövök."), "Szia\n\nHolnap jövök.");
        assert_eq!(c("first new paragraph new line second"), "First\n\n\nSecond");
        // A break at the very start still has nothing to attach to.
        assert_eq!(c("new line hello"), "New line hello");
        // Only breaks follow a break; punctuation words stay words.
        assert_eq!(c("hi new line comma there"), "Hi\nComma there");
        assert_eq!(c("I need a new line of credit."), "I need a new line of credit.");
    }

    #[test]
    fn spoken_punctuation_keeps_everyday_words() {
        // "pont" mid-sentence means "just/exactly".
        assert_eq!(c("pont úgy ahogy mondtad"), "Pont úgy ahogy mondtad");
        assert_eq!(c("ez pont jó"), "Ez pont jó");
        assert_eq!(c("Pont."), "Pont.");
        // Nouns after an article stay words.
        assert_eq!(c("kell egy új sor a táblázatba"), "Kell egy új sor a táblázatba");
        assert_eq!(c("hiányzik a vessző"), "Hiányzik a vessző");
        assert_eq!(c("it ends after the trial period"), "It ends after the trial period");
        assert_eq!(c("the period ended"), "The period ended");
        assert_eq!(c("nulla pont öt"), "Nulla pont öt");
        assert_eq!(c("a csapat 70 pont"), "A csapat 70 pont");
        // Ordinals and abbreviations keep their full stop.
        assert_eq!(c("a 3 kérdőjel"), "A 3 kérdőjel");
        assert_eq!(c("a Bosch Kft. kérdőjel"), "A Bosch Kft.?");
        // "2." stays an ordinal or a list number.
        assert_eq!(c("A 2. Világháború után"), "A 2. Világháború után");
        assert_eq!(c("Lista 1. Alma 2. Körte"), "Lista 1. Alma 2. Körte");
        assert_eq!(c("a verseny 2. helyezettje"), "A verseny 2. helyezettje");
        assert_eq!(c("Ez kettős pontosság"), "Ez kettős pontosság");
    }

    #[test]
    fn glued_spoken_comma_is_split() {
        // Whisper glues the misspelt "vesző" to its neighbours (spoken-01).
        assert_eq!(c("3 dolog az árvesző a határidő"), "3 dolog az ár, a határidő");
        assert_eq!(c("almaveszőkörteveszőszilva"), "Alma, körte, szilva");
        assert_eq!(c("az árvesző, a minőség"), "Az ár, a minőség");
        assert_eq!(c("Elküldtem pontosvesző holnap"), "Elküldtem; holnap");
        // Real words that contain "vesző" stay whole.
        assert_eq!(c("az elvesző adatok"), "Az elvesző adatok");
        assert_eq!(c("a szokás kiveszőben van"), "A szokás kiveszőben van");
        assert_eq!(c("sokat vesződtem vele"), "Sokat vesződtem vele");
        assert_eq!(c("a tizedesvessző helye"), "A tizedesvessző helye");
    }

    #[test]
    fn english_clock_time_takes_a_colon() {
        assert_eq!(c("The meeting is on March 15th at 3.30 p.m."), "The meeting is on March 15th at 3:30 p.m.");
        assert_eq!(c("call me at 10.05 am, please"), "Call me at 10:05 am, please");
        assert_eq!(c("Lunch at 12.15 PM."), "Lunch at 12:15 PM.");
        // Decimals, Hungarian times and dates stay as written.
        assert_eq!(c("it costs 3.30 dollars"), "It costs 3.30 dollars");
        assert_eq!(c("Fél 10.30-kor jövök."), "Fél 10.30-kor jövök.");
        assert_eq!(c("2026. november 7-én"), "2026. november 7-én");
        assert_eq!(c("at 13.30 pm"), "At 13.30 pm");
        assert_eq!(c("at 3.5 pm"), "At 3.5 pm");
    }

    #[test]
    fn keeps_all_caps_acronyms_that_spell_fillers() {
        assert_eq!(c("A HM közleménye szerint"), "A HM közleménye szerint");
        assert_eq!(c("The ER doctor."), "The ER doctor.");
        assert_eq!(c("UH is a university."), "UH is a university.");
        assert_eq!(c("HMM is a model."), "HMM is a model.");
        // Hesitations in ordinary case still go, and a capital "Ö" alone is still one.
        assert_eq!(c("Hmm, OK, hm."), "OK.");
        assert_eq!(c("Ö, holnap jövök."), "Holnap jövök.");
    }

    #[test]
    fn empty_and_only_fillers() {
        assert_eq!(c(""), "");
        assert_eq!(c("öö hmm"), "");
    }

    #[test]
    fn single_line_joins_lines_with_one_space() {
        assert_eq!(single_line("Szia, holnap jövök.\nHozol kávét?\n\nKöszi."), "Szia, holnap jövök. Hozol kávét? Köszi.");
        assert_eq!(single_line("Egy sor."), "Egy sor.");
        assert_eq!(single_line("\nElején és végén\n\n"), "Elején és végén");
        assert_eq!(single_line("a \r\n b"), "a b");
        assert_eq!(single_line(""), "");
    }
}
