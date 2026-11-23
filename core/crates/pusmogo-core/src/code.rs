//! Code style: the writing style for editors, IDEs and terminals.
//!
//! Text dictated into a code editor or a terminal is usually an identifier, a
//! command, a commit message or a prompt for a coding assistant, not prose. So
//! in the code category (`style::Category::Code`) the first letter is not
//! capitalised, no full stop is added or kept at the end, typographic quotes
//! become straight ones, and spoken case commands build identifiers:
//!
//! | Said (HU or EN)                         | Typed     |
//! |-----------------------------------------|-----------|
//! | "camel case user id"                    | `userId`  |
//! | "pascal case user id"                   | `UserId`  |
//! | "snake case user id"                    | `user_id` |
//! | "kebab case user id"                    | `user-id` |
//! | "constant case user id", "konstans case …" | `USER_ID` |
//!
//! A case command takes the words after it up to a punctuation mark, a short
//! joining word ("to", "and", "és", "a" …) or [`MAX_WORDS`] words, whichever
//! comes first, or before a later word with a non-ASCII letter (Hungarian
//! prose going on: "snake case user id névre" → "user_id névre"), so "rename camel case user id to camel case account id" gives
//! "rename userId to accountId". A Hungarian suffix written after a hyphen
//! stays on the identifier ("camel case user id-re" → "userId-re"). A bare "konstans" stays a word: it is an
//! everyday Hungarian adjective ("konstans érték").
//!
//! A spoken file tag mentions a file in a coding assistant's chat:
//! "tag index dot tsx", "címke index pont tsx" or "kukac index pont tsx" →
//! `@index.tsx`, "tag src slash app dot test dot ts" → `@src/app.test.ts`.
//! It only fires when the name ends in a known file extension
//! ([`EXTENSIONS`]), because "tag" (Hungarian "member") and "pont"
//! (Hungarian "exactly") are everyday words. Every rule is deterministic and
//! only reshapes words the user said.

/// The most words one case command joins into an identifier.
pub const MAX_WORDS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Camel,
    Pascal,
    Snake,
    Kebab,
    Constant,
}

/// Spoken commands, as lower-case words with hyphens removed. Longest first.
const COMMANDS: &[(&[&str], Case)] = &[
    (&["screaming", "snake", "case"], Case::Constant),
    (&["camel", "case"], Case::Camel),
    (&["pascal", "case"], Case::Pascal),
    (&["snake", "case"], Case::Snake),
    (&["kebab", "case"], Case::Kebab),
    (&["kebap", "case"], Case::Kebab),
    (&["constant", "case"], Case::Constant),
    (&["konstans", "case"], Case::Constant),
    (&["camelcase"], Case::Camel),
    (&["pascalcase"], Case::Pascal),
    (&["snakecase"], Case::Snake),
    (&["kebabcase"], Case::Kebab),
];

/// Short words that end an identifier: the user went on with the sentence.
const JOINERS: &[&str] = &[
    "to", "and", "or", "in", "is", "are", "the", "a", "an", "with", "from", "for", "of", "on", "at", "as", "into", "then", "than", "és",
    "vagy", "az", "egy", "hogy", "mint", "meg", "is", "de", "ha", "akkor", "nem", "helyett", "legyen", "lesz",
];

/// Joining words that also start common identifiers (`onClick`, `isLoading`,
/// `toString`, `inProgress`, `asJson`, `ofType`). Taken only as the first part.
const PREFIXES: &[&str] = &["is", "on", "to", "in", "as", "of"];

/// Hungarian suffixes written after a hyphen on a code word or a foreign
/// name ("userId-re", "user_id-t", "config-ban"). A word ending in one of these
/// after a hyphen ends the identifier and keeps its suffix as written.
const HYPHEN_SUFFIXES: &[&str] = &[
    "t", "at", "et", "ot", "öt", "k", "ak", "ek", "ok", "ök", "kat", "ket", "re", "ra", "ba", "be", "ban", "ben", "ból", "ből", "ról",
    "ről", "hoz", "hez", "höz", "nak", "nek", "nál", "nél", "tól", "től", "val", "vel", "on", "en", "ön", "n", "ig", "ért", "ként", "ja",
    "je", "ját", "jét", "jában", "jében",
];

/// Splits "id-re" into ("id", "-re") when the part after the last hyphen is a
/// Hungarian suffix.
fn hyphen_suffix(core: &str) -> Option<(&str, &str)> {
    let (stem, suffix) = core.rsplit_once('-')?;
    let ok = !stem.is_empty() && stem.is_ascii() && HYPHEN_SUFFIXES.contains(&suffix.to_lowercase().as_str());
    ok.then(|| (stem, &core[stem.len()..]))
}

/// Words that start a spoken file tag (typed as `@`).
const TAG_WORDS: &[&str] = &["tag", "címke", "kukac"];

/// Spoken separators inside a file path.
const DOT_WORDS: &[&str] = &["dot", "pont"];
const SLASH_WORDS: &[&str] = &["slash", "per", "perjel"];

/// File extensions a spoken file tag may end in. A tag only fires on one of
/// these, so "tag ott pont akkor" stays prose.
pub const EXTENSIONS: &[&str] = &[
    "c", "cc", "cfg", "cjs", "cpp", "cs", "css", "csv", "dart", "env", "go", "h", "hpp", "html", "ini", "ipynb", "java", "js", "json",
    "jsx", "kt", "lock", "lua", "md", "mdx", "mjs", "pdf", "php", "png", "py", "rb", "rs", "sh", "sql", "svelte", "svg", "swift", "toml",
    "ts", "tsx", "txt", "vue", "xml", "yaml", "yml", "zig",
];

/// From this many words on, text with no spoken command or file tag is prose.
const PROSE_WORDS: usize = 8;

/// Applies the code style to text already cleaned by `cleanup::clean`.
///
/// A message to a coding assistant is prose: it keeps its capital and its full
/// stop when it has [`PROSE_WORDS`] words or a second sentence, and nothing in
/// it was said as a case command or a file tag.
pub fn apply(text: &str) -> String {
    let text = straight_quotes(text.trim());
    let built = text.lines().map(|l| case_commands(&file_tags(l))).collect::<Vec<_>>().join("\n");
    if built == text && is_prose(&text) {
        return text;
    }
    terse(&text)
}

/// The code style without the prose exception, for the items of a list.
pub(crate) fn terse(text: &str) -> String {
    let text = straight_quotes(text.trim());
    let text = lower_start(&text);
    let text = text.lines().map(|l| case_commands(&file_tags(l))).collect::<Vec<_>>().join("\n");
    drop_final_stop(&text)
}

/// First words that make a long dictation a shell command, not prose.
const SHELL_WORDS: &[&str] = &[
    "git", "gh", "npm", "npx", "pnpm", "yarn", "bun", "node", "cd", "ls", "rm", "cp", "mv", "cat", "grep", "curl", "ssh", "sudo", "brew",
    "docker", "cargo", "python", "pip", "uv", "echo", "mkdir", "kubectl", "swift",
];

fn is_prose(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.first().is_some_and(|w| SHELL_WORDS.contains(&w.to_lowercase().as_str())) {
        return false;
    }
    let second_sentence = words.windows(2).any(|w| {
        w[0].ends_with(['.', '?', '!']) && !w[0].ends_with("..") && w[1].starts_with(char::is_uppercase)
    });
    words.len() >= PROSE_WORDS || second_sentence
}

fn straight_quotes(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '„' | '”' | '“' | '«' | '»' | '″' => '"',
            '‚' | '‘' | '’' | '′' => '\'',
            c => c,
        })
        .collect()
}

/// No capital at the start, unless the first word is an acronym or brand with
/// two capitals ("PDF", "README", "GitHub") or the English "I".
fn lower_start(text: &str) -> String {
    let first = text.split_whitespace().next().unwrap_or("");
    let word = first.trim_end_matches(|c: char| !c.is_alphanumeric());
    if word.chars().filter(|c| c.is_uppercase()).count() >= 2 || word == "I" || word.starts_with("I'") {
        return text.to_string();
    }
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// One trailing full stop goes ("run the tests." → "run the tests"); an
/// ellipsis stays.
fn drop_final_stop(text: &str) -> String {
    match text.strip_suffix('.') {
        Some(body) if !body.ends_with('.') => body.to_string(),
        _ => text.to_string(),
    }
}

fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase().replace('-', "")
}

/// The case command starting at `words[i]`, and how many words it takes.
fn command_at(words: &[&str], i: usize) -> Option<(Case, usize)> {
    COMMANDS.iter().find_map(|(spoken, case)| {
        let n = spoken.len();
        if i + n > words.len() {
            return None;
        }
        // Only the command's last word may carry punctuation ("camel case:" is not a command).
        let matches = spoken.iter().enumerate().all(|(k, w)| {
            let word = words[i + k];
            bare(word) == *w && (k + 1 == n || !word.ends_with([',', '.', ':', ';', '!', '?']))
        });
        matches.then_some((*case, n))
    })
}

fn case_commands(line: &str) -> String {
    let words: Vec<&str> = line.split(' ').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let Some((case, n)) = command_at(&words, i) else {
            out.push(glued_command(words[i]).unwrap_or_else(|| words[i].to_string()));
            i += 1;
            continue;
        };
        let cmd_end = words[i + n - 1];
        if cmd_end.ends_with([',', '.', ':', ';', '!', '?']) {
            // "… camel case." names the command, it does not use it.
            out.push(words[i..i + n].join(" "));
            i += n;
            continue;
        }
        let mut parts: Vec<String> = Vec::new();
        // Punctuation before the command ("\"camel case user id\"") stays in front.
        let first = words[i];
        let mut lead = first[..first.len() - first.trim_start_matches(|c: char| !c.is_alphanumeric()).len()].to_string();
        let mut trail = String::new();
        let mut j = i + n;
        while j < words.len() && parts.len() < MAX_WORDS {
            let word = words[j];
            if word.is_empty() || command_at(&words, j).is_some() {
                break;
            }
            let core = word.trim_matches(|c: char| !c.is_alphanumeric());
            // "user id-re": the suffix stays on the identifier after a hyphen.
            if let Some((stem, suffix)) = hyphen_suffix(core) {
                if parts.is_empty() {
                    lead += &word[..word.len() - word.trim_start_matches(|c: char| !c.is_alphanumeric()).len()];
                }
                parts.extend(stem.split(|c: char| !c.is_alphanumeric()).filter(|p| !p.is_empty()).map(str::to_lowercase));
                let rest = word.trim_end_matches(|c: char| !c.is_alphanumeric());
                trail = format!("{suffix}{}", &word[rest.len()..]);
                j += 1;
                break;
            }
            let b = core.to_lowercase();
            // A word with a non-ASCII letter after the first one is Hungarian
            // prose going on ("snake case user id névre"): identifiers are ASCII.
            // "camel case on click" → `onClick`: an English identifier prefix
            // is the first part when an ordinary word follows it.
            let prefix_first = parts.is_empty()
                && PREFIXES.contains(&b.as_str())
                && core.len() == word.len()
                && words.get(j + 1).is_some_and(|next| {
                    let nb = bare(next);
                    !nb.is_empty() && nb.is_ascii() && !JOINERS.contains(&nb.as_str()) && command_at(&words, j + 1).is_none()
                });
            if b.is_empty() || (JOINERS.contains(&b.as_str()) && !prefix_first) || (!parts.is_empty() && !b.is_ascii()) {
                break;
            }
            if parts.is_empty() {
                lead += &word[..word.len() - word.trim_start_matches(|c: char| !c.is_alphanumeric()).len()];
            }
            parts.extend(core.split(|c: char| !c.is_alphanumeric()).filter(|p| !p.is_empty()).map(str::to_lowercase));
            j += 1;
            let rest = word.trim_end_matches(|c: char| !c.is_alphanumeric());
            if rest.len() < word.len() {
                trail = word[rest.len()..].to_string();
                break;
            }
        }
        if parts.is_empty() {
            out.push(words[i..i + n].join(" "));
            i += n;
            continue;
        }
        out.push(format!("{lead}{}{trail}", join(&parts, case)));
        i = j;
    }
    out.join(" ")
}

/// A file name part: ASCII letters, digits, `-` and `_`.
fn is_name_part(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The extension of a file name already written with dots ("index.tsx").
pub(crate) fn has_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && EXTENSIONS.contains(&ext.to_lowercase().as_str()) && name.split(['.', '/']).all(is_name_part)
    })
}

/// A file name with a Hungarian case ending after a hyphen ("app.tsx-ben",
/// "main.rs-t"), or without one.
fn has_extension_or_ending(name: &str) -> bool {
    has_extension(name) || hyphen_suffix(name).is_some_and(|(stem, _)| has_extension(stem))
}

/// "tag index dot tsx" → "@index.tsx" (see the module docs).
fn file_tags(line: &str) -> String {
    let words: Vec<&str> = line.split(' ').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        match file_tag_at(&words, i) {
            Some((tag, next)) => {
                out.push(tag);
                i = next;
            }
            None => {
                out.push(words[i].to_string());
                i += 1;
            }
        }
    }
    out.join(" ")
}

/// Endings that make "tag…" an English word, not a glued tag ("tags.json",
/// "tagger.py", "tagline.md").
const TAG_WORD_ENDINGS: &[&str] = &["s", "ged", "ger", "gers", "ging", "line", "lines"];

/// Whisper often glues the tag word to the file name ("tagindex.tsx",
/// "kukacmain.rs", "címkeindex.tsx"): split it off again.
fn glued_file_tag(word: &str) -> Option<String> {
    let start = word.len() - word.trim_start_matches(|c: char| !c.is_alphanumeric()).len();
    let end = word.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
    let core = word.get(start..end)?;
    let lower = core.to_lowercase();
    let tag = TAG_WORDS.iter().find(|t| lower.starts_with(**t))?;
    let name = &core[core.char_indices().nth(tag.chars().count())?.0..];
    let stem = name.split(['.', '/']).next().unwrap_or("").to_lowercase();
    if !has_extension_or_ending(name) || (*tag == "tag" && TAG_WORD_ENDINGS.contains(&stem.as_str())) {
        return None;
    }
    Some(format!("{}@{name}{}", &word[..start], &word[end..]))
}

/// The typed tag and the index after it, when `words[i]` starts a file tag.
fn file_tag_at(words: &[&str], i: usize) -> Option<(String, usize)> {
    let tag_word = words[i];
    if let Some(tag) = glued_file_tag(tag_word) {
        return Some((tag, i + 1));
    }
    let lead = &tag_word[..tag_word.len() - tag_word.trim_start_matches(|c: char| !c.is_alphanumeric()).len()];
    // The tag word itself carries no punctuation after it ("tag," is prose).
    if !TAG_WORDS.contains(&tag_word[lead.len()..].to_lowercase().as_str()) {
        return None;
    }
    let first = *words.get(i + 1)?;
    let core_end = first.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
    // Already written as a file name: "tag index.tsx".
    if has_extension_or_ending(&first[..core_end]) {
        return Some((format!("{lead}@{}{}", &first[..core_end], &first[core_end..]), i + 2));
    }
    // Spoken: name (separator name)*, ending in a dot and a known extension.
    let mut path = String::new();
    let mut best: Option<(String, usize)> = None;
    let mut j = i + 1;
    let mut sep = "";
    loop {
        let word = *words.get(j)?;
        let end = word.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
        let (part, trail) = (&word[..end], &word[end..]);
        if !is_name_part(part) {
            break;
        }
        // An extension may carry a Hungarian case ending ("rs-t", "tsx-ben"),
        // which then closes the path.
        let (ext, ending) = match hyphen_suffix(part) {
            Some((stem, ending)) if sep == "." && EXTENSIONS.contains(&stem.to_lowercase().as_str()) => (stem, ending),
            _ => (part, ""),
        };
        let is_ext = sep == "." && EXTENSIONS.contains(&ext.to_lowercase().as_str());
        path += sep;
        path += &if is_ext { ext.to_lowercase() } else { part.to_string() };
        if is_ext {
            best = Some((format!("{lead}@{path}{ending}{trail}"), j + 1));
            if !ending.is_empty() {
                break;
            }
        }
        // Punctuation after a part ends the path.
        if !trail.is_empty() {
            break;
        }
        let Some(next) = words.get(j + 1) else { break };
        let next = next.to_lowercase();
        sep = if DOT_WORDS.contains(&next.as_str()) {
            "."
        } else if SLASH_WORDS.contains(&next.as_str()) {
            "/"
        } else {
            break;
        };
        j += 2;
    }
    best
}

/// Whisper sometimes writes the command and its words as one word
/// ("CamelCaseUserID"): a one-word command followed by capitalised humps.
fn glued_command(word: &str) -> Option<String> {
    let start = word.len() - word.trim_start_matches(|c: char| !c.is_alphanumeric()).len();
    let end = word.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
    if start >= end {
        return None;
    }
    let core = &word[start..end];
    let (case, rest) = COMMANDS.iter().filter(|(spoken, _)| spoken.len() == 1).find_map(|(spoken, case)| {
        let name = spoken[0];
        let prefix = core.get(..name.len())?;
        (prefix.to_lowercase() == name).then(|| (*case, &core[name.len()..]))
    })?;
    let rest = rest.trim_start_matches(['-', '_']);
    if !rest.chars().next().is_some_and(char::is_uppercase) {
        return None;
    }
    let parts = humps(rest);
    Some(format!("{}{}{}", &word[..start], join(&parts, case), &word[end..]))
}

/// "UserID" → ["user", "id"], "MaxRetryCount" → ["max", "retry", "count"].
fn humps(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut parts = Vec::new();
    let mut cur = String::new();
    for (k, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let prev = k.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(k + 1);
        let boundary = c.is_uppercase()
            && prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit() || (p.is_uppercase() && next.is_some_and(|n| n.is_lowercase())));
        if boundary && !cur.is_empty() {
            parts.push(std::mem::take(&mut cur));
        }
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn join(parts: &[String], case: Case) -> String {
    match case {
        Case::Camel => parts.iter().enumerate().map(|(k, p)| if k == 0 { p.clone() } else { capitalise(p) }).collect(),
        Case::Pascal => parts.iter().map(|p| capitalise(p)).collect(),
        Case::Snake => parts.join("_"),
        Case::Kebab => parts.join("-"),
        Case::Constant => parts.join("_").to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_commands_english() {
        assert_eq!(apply("Camel case user ID."), "userId");
        assert_eq!(apply("Pascal case user id"), "UserId");
        assert_eq!(apply("Snake case user id"), "user_id");
        assert_eq!(apply("Kebab-case user id."), "user-id");
        assert_eq!(apply("Constant case max retry count."), "MAX_RETRY_COUNT");
        assert_eq!(apply("Screaming snake case api key"), "API_KEY");
        assert_eq!(apply("camelcase get user name"), "getUserName");
    }

    #[test]
    fn glued_commands_from_the_engine() {
        // Whisper wrote "camel case user id" as one word in a TTS check.
        assert_eq!(apply("CamelCaseUserID"), "userId");
        assert_eq!(apply("Set SnakeCaseMaxRetryCount."), "set max_retry_count");
        assert_eq!(apply("PascalCaseHTTPServer"), "HttpServer");
        // A word that merely starts like a command stays.
        assert_eq!(apply("Camelcase is a style."), "camelcase is a style");
    }

    #[test]
    fn case_commands_hungarian() {
        assert_eq!(apply("Konstans case user id."), "USER_ID");
        assert_eq!(apply("Nevezd át camel case user id és snake case account id."), "nevezd át userId és account_id");
        // An inflected Hungarian word after the identifier is not part of it.
        assert_eq!(apply("Nevezd át snake case user id névre."), "nevezd át user_id névre");
        assert_eq!(apply("Camel case max retry count értékét növeld."), "maxRetryCount értékét növeld");
        // A Hungarian first word is still taken when the user asks for it.
        assert_eq!(apply("Snake case felhasználó id"), "felhasználó_id");
    }

    #[test]
    fn identifier_stops_at_joiners_and_punctuation() {
        assert_eq!(apply("Rename camel case user id to camel case account id."), "rename userId to accountId");
        assert_eq!(apply("Set snake case retry count, then run the tests."), "set retry_count, then run the tests");
        assert_eq!(apply("Use \"camel case user id\" here."), "use \"userId\" here");
        // Never more than MAX_WORDS words.
        assert_eq!(apply("camel case one two three four five six"), "oneTwoThreeFourFive six");
    }

    #[test]
    fn an_identifier_may_start_with_an_english_prefix() {
        assert_eq!(apply("Camel case on click"), "onClick");
        assert_eq!(apply("Set camel case is loading to true."), "set isLoading to true");
        assert_eq!(apply("Call camel case to string."), "call toString");
        assert_eq!(apply("Snake case in progress"), "in_progress");
        // A prefix before a joiner, a command or nothing is still prose.
        assert_eq!(apply("Camel case and pascal case"), "camel case and pascal case");
        assert_eq!(apply("Camel case is a style."), "camel case is a style");
        assert_eq!(apply("Camel case to snake case user id"), "camel case to user_id");
        assert_eq!(apply("Write it in camel case in"), "write it in camel case in");
    }

    #[test]
    fn a_command_without_words_stays_as_said() {
        assert_eq!(apply("What is camel case?"), "what is camel case?");
        assert_eq!(apply("Prefer snake case."), "prefer snake case");
        assert_eq!(apply("A konstans érték nem változik."), "a konstans érték nem változik");
    }

    #[test]
    fn no_capital_no_final_stop_straight_quotes() {
        assert_eq!(apply("Futtasd le a teszteket."), "futtasd le a teszteket");
        assert_eq!(apply("Azt írja ki, hogy „kész”."), "azt írja ki, hogy \"kész\"");
        assert_eq!(apply("It says “done” and it’s fine."), "it says \"done\" and it's fine");
        assert_eq!(apply("README frissítése."), "README frissítése");
        assert_eq!(apply("I think it works."), "I think it works");
        assert_eq!(apply("Wait..."), "wait...");
        assert_eq!(apply("Mi a hiba?"), "mi a hiba?");
    }

    #[test]
    fn prose_to_an_assistant_keeps_its_capital_and_full_stop() {
        let long = "Also, you are not doing a lot of making the sentence nicer.";
        assert_eq!(apply(long), long);
        assert_eq!(apply("Ez nem jó. Javítsd ki."), "Ez nem jó. Javítsd ki.");
        assert_eq!(apply("Azt írja ki, hogy „kész”, de szerintem ez még nem jó."), "Azt írja ki, hogy \"kész\", de szerintem ez még nem jó.");
        // A long shell command is not prose; a short one-sentence line stays terse.
        assert_eq!(apply("Git commit all the changes with a message about login."), "git commit all the changes with a message about login");
        // Edge cases: empty, only punctuation, an ellipsis, Hungarian accents, a question.
        assert_eq!(apply(""), "");
        assert_eq!(apply("..."), "...");
        assert_eq!(apply("Wait... what"), "wait... what");
        assert_eq!(apply("Őszintén szólva ez így még nem elég jó nekünk."), "Őszintén szólva ez így még nem elég jó nekünk.");
        assert_eq!(apply("Mikor lesz kész?"), "mikor lesz kész?");
        // Short commands, case commands and file tags stay terse.
        assert_eq!(apply("Run the tests."), "run the tests");
        assert_eq!(apply("Please rename camel case user id to camel case account id."), "please rename userId to accountId");
        assert_eq!(apply("Please look at the tag index dot tsx and fix it."), "please look at the @index.tsx and fix it");
    }

    #[test]
    fn spoken_file_tags() {
        assert_eq!(apply("Tag index dot tsx."), "@index.tsx");
        assert_eq!(apply("Címke index pont tsx."), "@index.tsx");
        assert_eq!(apply("Nézd meg a kukac index pont tsx fájlt."), "nézd meg a @index.tsx fájlt");
        assert_eq!(apply("Look at tag index dot TSX and fix it."), "look at @index.tsx and fix it");
        assert_eq!(apply("Tag src slash App dot test dot ts, please."), "@src/App.test.ts, please");
        assert_eq!(apply("Javítsd a címke src per main pont rs fájlban."), "javítsd a @src/main.rs fájlban");
        // Whisper wrote the file name itself.
        assert_eq!(apply("Tag package.json."), "@package.json");
        // The longest path ending in an extension; a later word stays.
        assert_eq!(apply("tag index dot ts dot bak"), "@index.ts dot bak");
        // Glued by the engine (large-v3 and turbo on TTS clips).
        assert_eq!(apply("Look at tagindex.tsx and fix the import."), "look at @index.tsx and fix the import");
        assert_eq!(apply("Nézd meg a címkeindex.tsx fájlt."), "nézd meg a @index.tsx fájlt");
        assert_eq!(apply("Javítsd ki a kukacmain.rs fájlban a hibát."), "javítsd ki a @main.rs fájlban a hibát");
    }

    #[test]
    fn a_file_tag_keeps_a_hungarian_case_ending() {
        assert_eq!(apply("Nézd meg a kukac main pont rs-t."), "nézd meg a @main.rs-t");
        assert_eq!(apply("A hiba a címke app pont tsx-ben van."), "a hiba a @app.tsx-ben van");
        assert_eq!(apply("A hiba a kukac app.tsx-ben van."), "a hiba a @app.tsx-ben van");
        assert_eq!(apply("Javítsd a kukacmain.rs-ben a hibát."), "javítsd a @main.rs-ben a hibát");
        assert_eq!(apply("Nyisd meg a címke src per App pont TSX-et."), "nyisd meg a @src/App.tsx-et");
        // The ending closes the path, and a word after it stays.
        assert_eq!(apply("kukac main pont rs-t pont most"), "@main.rs-t pont most");
        assert_eq!(apply("Tag main dot rs, then run it."), "@main.rs, then run it");
        // Not a case ending, or not an extension: prose.
        assert_eq!(apply("kukac main pont rs-xyz"), "kukac main pont rs-xyz");
        assert_eq!(apply("kukac main pont foo-t"), "kukac main pont foo-t");
    }

    #[test]
    fn file_tag_words_in_prose_stay() {
        // "tag" is Hungarian for "member", "pont" for "exactly".
        assert_eq!(apply("A tag pont most jött."), "a tag pont most jött");
        assert_eq!(apply("Ő új tag ott pont akkor."), "ő új tag ott pont akkor");
        assert_eq!(apply("Tag the release, then deploy."), "tag the release, then deploy");
        assert_eq!(apply("Tag, pont tsx."), "tag, pont tsx");
        assert_eq!(apply("Tag index dot html5"), "tag index dot html5");
        assert_eq!(apply("Tag version.2"), "tag version.2");
        // English words that start with "tag" stay file names.
        assert_eq!(apply("Open tags.json and tagger.py."), "open tags.json and tagger.py");
        assert_eq!(apply("Edit tagline.md"), "edit tagline.md");
        assert_eq!(apply("Read tag.md"), "read tag.md");
    }

    #[test]
    fn keeps_lines() {
        assert_eq!(apply("Első sor.\nMásodik sor camel case user id."), "első sor.\nMásodik sor userId");
    }

    #[test]
    fn a_hyphenated_hungarian_suffix_stays_on_the_identifier() {
        assert_eq!(apply("Nevezd át camel case user id-re."), "nevezd át userId-re");
        assert_eq!(apply("Töröld a snake case user id-t."), "töröld a user_id-t");
        assert_eq!(apply("Nézd meg a pascal case app config-ban."), "nézd meg a AppConfig-ban");
        assert_eq!(apply("Keresd a konstans case max size-ról, aztán futtasd."), "keresd a MAX_SIZE-ról, aztán futtasd");
        // A hyphen that is not a suffix is still part of the name.
        assert_eq!(apply("camel case user-name field"), "userNameField");
        assert_eq!(apply("kebab case log-in page"), "log-in-page");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(apply("  "), "");
    }
}
