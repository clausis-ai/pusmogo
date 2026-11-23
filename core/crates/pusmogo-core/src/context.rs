//! Context hints: name-like words from the text around the cursor, so the
//! engine spells the names already visible in the field ("Bencének" after
//! "Kovács Bence" on screen). The app reads the text through Accessibility
//! just before a dictation; it is only held in memory for that dictation and
//! never stored.

/// Longest stretch of context text looked at, in characters, counted back from
/// the cursor. Names near the cursor matter most; the rest is cut.
pub const MAX_CONTEXT_CHARS: usize = 2000;

/// Most hints (words or full names) passed to the engine, so the personal dictionary keeps most
/// of Whisper's 224-token prompt window.
pub const MAX_HINT_WORDS: usize = 16;

/// Context hints go into the engine prompt only when the dictation holds at
/// least this much voiced audio. On a short phrase the prompt outweighs the
/// sound: English hints from a terminal or a chat turned "Szia" into "See ya"
/// and "Lámpa" into "Lampa" (docs/bench/prompt-01.md). Names in short
/// dictations are still fixed by [`respell`], which runs on every length.
pub const MIN_VOICED_SECS_FOR_HINTS: f32 = 2.0;

/// The hints to put into the prompt for a dictation of `samples` (16 kHz
/// mono): all of them, or none when the dictation is shorter than
/// [`MIN_VOICED_SECS_FOR_HINTS`] of voiced audio.
pub fn prompt_hints<'a>(hints: &'a [String], samples: &[f32]) -> &'a [String] {
    if hints.is_empty() || crate::vad::VoiceActivity::analyze(samples).voiced_secs() < min_voiced_secs() {
        return &[];
    }
    hints
}

/// [`MIN_VOICED_SECS_FOR_HINTS`], or `PUSMOGO_HINT_MIN_VOICED` for A/B runs
/// (0 puts hints into every prompt, as before the threshold).
fn min_voiced_secs() -> f32 {
    std::env::var("PUSMOGO_HINT_MIN_VOICED").ok().and_then(|v| v.parse().ok()).unwrap_or(MIN_VOICED_SECS_FOR_HINTS)
}

const EDGE_PUNCT: &[char] = &[
    ',', '.', '!', '?', ';', ':', '…', '"', '\'', '(', ')', '[', ']', '{', '}', '<', '>', '„', '”', '“', '»', '«', '’',
    '‘', '*', '_', '`', '–', '—', '-', '/', '|',
];

/// Name-like words in `text`, adjacent ones joined into one hint ("Kovács
/// Bence"), nearest the end (the cursor) first kept, at most `max`, returned
/// in reading order. A word counts when it is capitalised
/// mid-sentence ("Bence"), has a capital after its first letter ("iPhone",
/// "API", "McKinsey") or mixes letters and digits ("GPT-4"). Words that only
/// appear capitalised at the start of a sentence are skipped: there a capital
/// says nothing about the word, unless a name follows it ("Szűts Ottó
/// írta.", a surname first) and it is not a common opener ("The", "Kedves").
pub fn hint_words(text: &str, max: usize) -> Vec<String> {
    scan(text, max, false)
}

/// [`hint_words`] for a code editor or terminal: also identifiers written in
/// snake or constant case ("user_id", "MAX_RETRY") and file names with a known
/// extension ("index.tsx", "src/main.rs"), each a hint of its own. Elsewhere
/// they stay out: in prose fields they are rare, and a technical word in the
/// prompt can pull everyday speech toward it.
pub fn code_hint_words(text: &str, max: usize) -> Vec<String> {
    scan(text, max, true)
}

fn scan(text: &str, max: usize, code: bool) -> Vec<String> {
    let start = text.char_indices().rev().nth(MAX_CONTEXT_CHARS - 1).map_or(0, |(i, _)| i);
    let text = &text[start..];
    let mut found: Vec<String> = Vec::new();
    for line in text.split('\n') {
        // A cut-off first word may be a sentence start too: treat it as one.
        let mut sentence_start = true;
        let mut phrase: Vec<&str> = Vec::new();
        // A capitalised sentence opener, kept only if a name follows it.
        let mut opener: Option<&str> = None;
        for raw in line.split_whitespace() {
            if code {
                // Code glues names to brackets and operators: "load(user_id:".
                let ids: Vec<&str> = raw.split(|c: char| "()[]{}<>=,;\"'`".contains(c)).filter_map(identifier).collect();
                if !ids.is_empty() {
                    push_phrase(&mut found, &mut phrase);
                    for id in ids {
                        push_phrase(&mut found, &mut vec![id]);
                    }
                    opener = None;
                    sentence_start = false;
                    continue;
                }
            }
            let word = without_clitic(raw.trim_matches(EDGE_PUNCT));
            if !word.is_empty() && is_name_like(word, sentence_start) {
                // "Szűts Ottó" opening a sentence: Hungarian puts the surname
                // first, so a name right after the opener makes it the surname.
                phrase.extend(opener.take());
                phrase.push(word);
            } else {
                push_phrase(&mut found, &mut phrase);
            }
            opener = None;
            let punct_after = raw.trim_end_matches(EDGE_PUNCT).len() < raw.len();
            if sentence_start && !punct_after && is_opener_name(word) {
                opener = Some(word);
            }
            // Punctuation after a word ends the phrase: "Bence, Dóra" are two.
            if punct_after {
                push_phrase(&mut found, &mut phrase);
            }
            sentence_start = raw.trim_end_matches(['"', '\'', ')', '”', '»', '’']).ends_with(['.', '!', '?', ':']);
        }
        push_phrase(&mut found, &mut phrase);
    }
    let keep = found.len().saturating_sub(max);
    found.split_off(keep)
}

/// A code identifier or file name in `raw` (surrounding punctuation, quotes
/// and backticks cut): "user_id", "MAX_RETRY", "index.tsx", "src/main.rs".
/// Plain words, camel case (already a name-like word) and leading-underscore
/// names stay with the general rules.
fn identifier(raw: &str) -> Option<&str> {
    let word = raw.trim_matches(|c: char| EDGE_PUNCT.contains(&c) && c != '_' && c != '/' || c == '`');
    let word = word.trim_end_matches(['.', ',', ':', ';']).trim_start_matches('/');
    if word.chars().filter(|c| c.is_alphabetic()).count() < 2 {
        return None;
    }
    let snake = word.contains('_')
        && !word.starts_with('_')
        && !word.ends_with('_')
        && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    (snake || crate::code::has_extension(word)).then_some(word)
}

/// Adds adjacent name-like words as one hint ("Kovács Bence"): Whisper copies
/// the prompt's commas, so a full name split into "Kovács, Bence" came back
/// with a comma inside it. A repeated hint moves to its latest place, nearest
/// the cursor.
fn push_phrase(found: &mut Vec<String>, phrase: &mut Vec<&str>) {
    if phrase.is_empty() {
        return;
    }
    let hint = phrase.join(" ");
    phrase.clear();
    found.retain(|w| *w != hint);
    found.push(hint);
}

/// An English clitic cut off: "Péter's" → "Péter", "I'm" → "I". A hint
/// with "'s" made the prompt and the respelling miss every other form, and
/// "I'm" or "I'll" mid-sentence are not names. "O'Brien" keeps its apostrophe.
fn without_clitic(word: &str) -> &str {
    let Some(i) = word.rfind(['\'', '’']) else { return word };
    let clitic = word[i..].trim_start_matches(['\'', '’']);
    if ["s", "m", "ll", "ve", "d", "re"].contains(&clitic) { &word[..i] } else { word }
}

/// Sentence openers that are never the first half of a name: articles,
/// determiners, pronouns and greetings ("The Clausis team", "Kedves Anna").
const NOT_OPENER_NAMES: &[&str] = &[
    "a", "az", "egy", "ez", "ezt", "azt", "ha", "és", "de", "kedves", "szia", "sziasztok", "szervusz", "üdv", "the",
    "an", "my", "our", "your", "his", "her", "their", "its", "this", "that", "these", "those", "dear", "hi", "hello",
    "hey", "thanks", "and", "but", "if", "so", "we", "you", "they", "he", "she", "it",
];

/// A sentence's first word that could be a surname: capitalised, all letters,
/// at least 3 of them, and not a common opener.
fn is_opener_name(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
        && word.chars().all(char::is_alphabetic)
        && word.chars().count() >= 3
        && !NOT_OPENER_NAMES.contains(&word.to_lowercase().as_str())
}

fn is_name_like(word: &str, sentence_start: bool) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 || word.chars().any(|c| !(c.is_alphanumeric() || "-.'’&".contains(c))) {
        return false;
    }
    let mut chars = word.chars();
    let first = chars.next().unwrap_or(' ');
    let inner_capital = chars.any(char::is_uppercase);
    let digit = word.chars().any(|c| c.is_ascii_digit());
    inner_capital || digit || (first.is_uppercase() && !sentence_start)
}

/// Fixes near-miss spellings of hint names in the engine's raw output, also in
/// inflected forms: with "Szűts Ottóval" on screen, "Szücs Otónak" becomes
/// "Szűts Ottónak". The prompt mostly fixes nominative forms; a suffix the
/// prompt did not show often brings the common spelling back. Two words match
/// when they agree letter by letter up to accents, doubled letters, "cz"/"c"
/// and "ts"/"cs" over the hint's whole stem: the hint may end in at most 4
/// other letters (its own suffix), the output word in at most 6. The stem
/// comes from the hint and the suffix from the output, with the output's vowel
/// at the joint (a suffix lengthens "Bence" to "Bencé-"). Only capitalised
/// output words change, and a sentence's first word only on a stem of 5
/// letters or more, so "Kis" opening a sentence never becomes a hinted "Kiss".
pub fn respell(raw: &str, hints: &[String]) -> String {
    let names: Vec<Vec<char>> = hints
        .iter()
        .flat_map(|h| h.split_whitespace())
        .filter(|w| {
            let mut cs = w.chars();
            cs.next().is_some_and(char::is_uppercase)
                && w.chars().all(char::is_alphabetic)
                && w.chars().skip(1).any(char::is_lowercase)
                && w.chars().count() >= 4
        })
        .map(|w| w.chars().collect())
        .collect();
    if names.is_empty() {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut sentence_start = true;
    let mut rest = raw;
    while !rest.is_empty() {
        let ws = rest.len() - rest.trim_start().len();
        out.push_str(&rest[..ws]);
        rest = &rest[ws..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let raw_word = &rest[..end];
        rest = &rest[end..];
        let lead = raw_word.len() - raw_word.trim_start_matches(EDGE_PUNCT).len();
        let whole = raw_word.trim_matches(EDGE_PUNCT);
        // Respell the name and keep an English clitic: "Otto's" → "Ottó's".
        let word = without_clitic(whole);
        let tail = &raw_word[lead + word.len()..];
        let fixed = word
            .chars()
            .next()
            .filter(|c| c.is_uppercase() && word.chars().all(char::is_alphabetic))
            .and_then(|_| {
                let w: Vec<char> = word.chars().collect();
                // A sentence's first word gains a final "h" only in a full
                // name: "Német Anna" is "Németh Anna", "Német autót" stays.
                let final_h = !sentence_start || rest.trim_start().starts_with(char::is_uppercase);
                names.iter().find_map(|n| respell_word(n, &w, if sentence_start { 5 } else { 4 }, final_h))
            });
        out.push_str(&raw_word[..lead]);
        out.push_str(fixed.as_deref().unwrap_or(word));
        out.push_str(tail);
        sentence_start = raw_word.trim_end_matches(['"', '\'', ')', '”', '»', '’']).ends_with(['.', '!', '?', ':']);
    }
    out
}

/// `word` respelt after `name`, or `None` when they do not match or nothing changes.
fn respell_word(name: &[char], word: &[char], min_stem: usize, final_h: bool) -> Option<String> {
    let (mut i, mut j) = (0, 0);
    let mut name_doubled = false;
    // Letters matched since the name letter the output dropped, if it dropped one.
    let mut after_drop: Option<usize> = None;
    while i < name.len() && j < word.len() {
        let (a, b) = (base(name[i]), base(word[j]));
        if a == b {
            i += 1;
            j += 1;
            name_doubled = false;
            after_drop = after_drop.map(|n| n + 1);
        } else if i > 0 && a == base(name[i - 1]) {
            i += 1; // doubled letter in the name: "Ottó" against "Otó"
            name_doubled = true;
        } else if j > 0 && b == base(word[j - 1]) {
            j += 1;
        } else if a == 'z' && i > 0 && base(name[i - 1]) == 'c' {
            i += 1; // "Bencze" against "Bence"
        } else if b == 'z' && j > 0 && base(word[j - 1]) == 'c' {
            j += 1;
        } else if matches!((a, b), ('t', 'c') | ('c', 't')) && name.get(i + 1).map(|&c| base(c)) == Some('s') {
            i += 1; // "Szűts" against "Szücs"
            j += 1;
        } else if final_h && old_spelling_h(name, i) {
            i += 1; // "Balogh" against "Balognak"
        } else if after_drop.is_none() && i >= 2 && name.len() >= 5 && name.get(i + 1).map(|&c| base(c)) == Some(b) {
            i += 1; // one letter dropped: "Szűts" against "Szts"
            after_drop = Some(0);
        } else {
            break;
        }
    }
    // A dropped letter must sit inside the name, with at least two matching
    // letters after it, so "Petra" never becomes "Péter".
    if after_drop.is_some_and(|n| n < 2 || i < name.len()) {
        return None;
    }
    // A doubled letter just before the hint's own suffix belongs to that
    // suffix: "Péterrel" against "Péternek" has the stem "Péter".
    if name_doubled && i < name.len() {
        i -= 1;
    }
    // The name's doubled last letter written once: "Kiss" against "Kis" (but
    // "Péterrel" against "Péter" doubles only because of its suffix).
    if j == word.len() && i > 0 && i + 1 == name.len() && base(name[i]) == base(name[i - 1]) {
        i += 1;
    }
    // An old-spelling "-gh" or "-th" heard without its "h": "Balogh" against "Balog".
    if final_h && j == word.len() && old_spelling_h(name, i) {
        i += 1;
    }
    if i < min_stem || name.len() - i > 4 || word.len() - j > 6 {
        return None;
    }
    let mut fixed: Vec<char> = name[..i].to_vec();
    // A suffix lengthens a final "a" or "e" ("Bencé-vel", "Lillá-val"), and
    // the suffix kept is the output's, so its vowel at the joint is the right one.
    if j > 0 && matches!(base(word[j - 1]), 'a' | 'e') {
        fixed[i - 1] = word[j - 1];
    }
    fixed.extend_from_slice(&word[j..]);
    (fixed != word).then(|| fixed.into_iter().collect())
}

/// Whether `name[i]` is the final "h" of an old-spelling "-gh"/"-th" surname
/// of 5 letters or more ("Balogh", "Horváth"; not "Tóth").
fn old_spelling_h(name: &[char], i: usize) -> bool {
    i >= 4 && i + 1 == name.len() && base(name[i]) == 'h' && matches!(base(name[i - 1]), 'g' | 't')
}

/// A letter without case or accents.
fn base(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'á' => 'a',
        'é' => 'e',
        'í' => 'i',
        'ó' | 'ö' | 'ő' => 'o',
        'ç' => 'c', // Whisper's "Gyuriça" for "Gyuricza"
        'ú' | 'ü' | 'ű' => 'u',
        l => l,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_mid_sentence_names_and_skips_sentence_starts() {
        let text = "Szia! Holnap találkozom Kovács Bencével. Utána megyek a Clausis irodába.";
        assert_eq!(hint_words(text, 12), ["Kovács Bencével", "Clausis"]);
    }

    #[test]
    fn a_full_name_opening_a_sentence_keeps_its_surname() {
        let text = "Megvan a szerződés. Szűts Ottó és Bencze Lilla átnézik. The Clausis team.\nDear Anna,\nKedves Péter jön.";
        assert_eq!(hint_words(text, 12), ["Szűts Ottó", "Bencze Lilla", "Clausis", "Anna", "Péter"]);
        // An opener alone, or cut off by punctuation, is still skipped.
        assert_eq!(hint_words("Holnap jövök. Kovács, Bence is.", 12), ["Bence"]);
    }

    #[test]
    fn keeps_inner_capitals_acronyms_and_digits() {
        let text = "Az iPhone és a GPT-4 is ott volt, meg az API.";
        assert_eq!(hint_words(text, 12), ["iPhone", "GPT-4", "API"]);
    }

    #[test]
    fn punctuation_splits_names_into_separate_hints() {
        let text = "ott volt Szűts Dóra, Rácz Benedek és a Clausis csapat.";
        assert_eq!(hint_words(text, 12), ["Szűts Dóra", "Rácz Benedek", "Clausis"]);
    }

    #[test]
    fn line_breaks_start_a_sentence() {
        let text = "Kedves Anna,\nKöszönöm a választ.\nÜdv";
        assert_eq!(hint_words(text, 12), ["Anna"]);
    }

    #[test]
    fn nearest_words_win_and_stay_in_reading_order() {
        let text = "a Bence, a Dóra, a Péter és a Zsófi, meg a Bence";
        assert_eq!(hint_words(text, 2), ["Zsófi", "Bence"]);
    }

    #[test]
    fn deduplicates_and_ignores_urls_and_single_letters() {
        let text = "a Bence és a Bence, lásd https://example.com vagy X";
        assert_eq!(hint_words(text, 12), ["Bence"]);
    }

    #[test]
    fn looks_only_at_the_text_near_the_cursor() {
        let far = format!("a Messzi {}", "szó ".repeat(MAX_CONTEXT_CHARS));
        assert_eq!(hint_words(&format!("{far}a Közeli"), 12), ["Közeli"]);
    }

    #[test]
    fn short_dictations_leave_hints_out_of_the_prompt() {
        let h = hints(&["Kovács Bence"]);
        // Alternating loud and quiet 30 ms frames read as speech, so half the
        // audio counts as voiced.
        let speech = |secs: f32| -> Vec<f32> {
            (0..(secs * 16_000.0) as usize).map(|i| if (i / 480) % 2 == 0 { 0.3 * ((i as f32) * 0.05).sin() } else { 0.0 }).collect()
        };
        assert!(prompt_hints(&h, &speech(2.0)).is_empty(), "1 s voiced");
        assert_eq!(prompt_hints(&h, &speech(5.0)), &h[..], "2.5 s voiced");
        assert!(prompt_hints(&[], &speech(5.0)).is_empty());
    }

    fn hints(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn respells_hint_names_also_inflected() {
        let h = hints(&["Szűts Ottóval", "Bencze Lillával", "Gyuricza Péterrel", "Clausis"]);
        assert_eq!(respell("Holnap Szücs Otónak írok.", &h), "Holnap Szűts Ottónak írok.");
        assert_eq!(respell("Holnap Szücs Otto és Bence Lila is eljön.", &h), "Holnap Szűts Ottó és Bencze Lilla is eljön.");
        assert_eq!(respell("pedig Gyurica Péter is.", &h), "pedig Gyuricza Péter is.");
        assert_eq!(respell("Gyuriça Péternek küldtem.", &h), "Gyuricza Péternek küldtem.");
        assert_eq!(respell("beszéltem Szücs Otóval.", &h), "beszéltem Szűts Ottóval.");
        assert_eq!(respell("a Clausis, a Bencének", &h), "a Clausis, a Benczének");
    }

    #[test]
    fn respells_a_name_missing_one_letter() {
        let h = hints(&["Szűts Ottó", "Kowalczyk", "Péter"]);
        assert_eq!(respell("Szts Ottó's team moved.", &h), "Szűts Ottó's team moved.");
        assert_eq!(respell("írtam Sztsnek és a Kowalcyk", &h), "írtam Szűtsnek és a Kowalczyk");
        // Too little left after the gap, or the gap at the name's start.
        for text in ["a Petra szerint", "a Petrának", "láttam Zűts", "a Kowlaczyk"] {
            assert_eq!(respell(text, &h), text);
        }
    }

    #[test]
    fn respells_an_old_spelling_name_missing_its_final_h() {
        let h = hints(&["Balogh Ádám", "Horváth Réka", "Tóth"]);
        assert_eq!(respell("Szia Balog Ádám!", &h), "Szia Balogh Ádám!");
        assert_eq!(respell("Horvát Réka holnap jön.", &h), "Horváth Réka holnap jön.");
        assert_eq!(respell("írtam Horvátnak és Balognak", &h), "írtam Horváthnak és Baloghnak");
        // A nationality word opening a sentence is not a name.
        let h = hints(&["Németh Anna", "Horváth Réka"]);
        assert_eq!(respell("Német Anna jön.", &h), "Németh Anna jön.");
        for text in ["Német autót vettem.", "Horvát tengerpartra megyünk.", "Igen. Német sört kérek."] {
            assert_eq!(respell(text, &h), text);
        }
        let h = hints(&["Balogh Ádám", "Horváth Réka", "Tóth"]);
        // Already right, a 4-letter hint, and the reverse direction stay as said.
        for text in ["Szia Balogh Ádám!", "a Tót utca", "a Balaton partján"] {
            assert_eq!(respell(text, &h), text);
        }
        let plain = hints(&["Horvát"]);
        for text in ["láttam Horváthot", "láttam Horvátot"] {
            assert_eq!(respell(text, &plain), text);
        }
    }

    #[test]
    fn leaves_other_words_alone() {
        let h = hints(&["Kiss", "Szabó", "Hollandia", "Péter", "Kertai"]);
        for text in ["Kis kutya ugat.", "Szabad vagy.", "Holnap jövök.", "a Petra szerint", "a Kertész utca", "API és GPT-4", "kiss szabad"] {
            assert_eq!(respell(text, &h), text);
        }
        assert_eq!(respell("Szücs Otó", &[]), "Szücs Otó");
        // Mid-sentence a 4-letter stem is enough.
        assert_eq!(respell("láttam Kisst", &hints(&["Kiss"])), "láttam Kisst");
        assert_eq!(respell("láttam a Kis nevű", &hints(&["Kiss"])), "láttam a Kiss nevű");
    }

    #[test]
    fn code_hints_take_identifiers_and_file_names() {
        let text = "fn load(user_id: u32) -> Result<User> {\n    let MAX_RETRY = 3; // see src/main.rs and `index.tsx`.\n    retryCount += 1;";
        assert_eq!(
            code_hint_words(text, 16),
            ["user_id", "MAX_RETRY", "src/main.rs", "index.tsx", "retryCount"]
        );
        // The window title of an editor names the open file.
        assert_eq!(code_hint_words("session.rs — pusmogo", 16), ["session.rs"]);
        // Prose fields keep the general rules: no snake case, no lower-case file names.
        assert!(hint_words("nézd meg a user_id mezőt az index.tsx fájlban", 16).is_empty());
        // Not identifiers: a lone underscore name, a version, an unknown extension, plain words.
        assert!(code_hint_words("_private __init__ v1.2 notes.xyz a_ b mappa", 16).is_empty());
    }

    #[test]
    fn empty_text_gives_nothing() {
        assert!(hint_words("", 12).is_empty());
    }

    #[test]
    fn english_clitics_are_cut_from_hints() {
        let text = "I spoke with Aoife Brennan, and I'm sure Gyuricza Péter's invoice and O'Brien are fine. I'll ask Ottó’s team.";
        assert_eq!(hint_words(text, 12), ["Aoife Brennan", "Gyuricza Péter", "O'Brien", "Ottó"]);
    }

    #[test]
    fn respells_names_before_an_english_clitic() {
        let h = hints(&["Ottó", "Bencze Lilla"]);
        assert_eq!(respell("I forwarded Otto's notes to Bence Lila.", &h), "I forwarded Ottó's notes to Bencze Lilla.");
        assert_eq!(respell("so Lila’s draft", &h), "so Lilla’s draft");
        assert_eq!(respell("I'm sure it's fine.", &h), "I'm sure it's fine.");
    }
}
