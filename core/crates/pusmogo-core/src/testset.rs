//! Test-set recording mode: the reading script, the order of takes and where
//! each take is saved.
//!
//! A speaker reads every line of `bench/testset/script.tsv` once in a normal
//! voice, then the lines marked `whisper=yes` a second time whispering. Each
//! take is saved in the speaker's folder as `<id>.wav` or `<id>-whisper.wav`
//! (16 kHz mono, 16-bit) with its reference text beside it (`.txt`), the
//! layout `scripts/testset-manifest.py` reads. The speaker's consent level goes
//! into `consent.txt` (see `bench/testset/CONSENT.md`). Nothing leaves the Mac.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::profile::json_str;
use crate::SAMPLE_RATE;

/// The reading script, compiled in so the app needs no extra file.
pub const SCRIPT: &str = include_str!("../../../../bench/testset/script.tsv");

/// Takes shorter than this are refused, as the manifest builder skips them.
pub const MIN_TAKE_SECS: f32 = 0.3;

/// One line of the reading script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub id: String,
    pub lang: String,
    pub category: String,
    pub whisper: bool,
    pub text: String,
}

/// One recording to make: a script line, read normally or whispered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Take {
    pub line: Line,
    pub whispered: bool,
}

impl Take {
    /// File name stem in the speaker folder: `<id>` or `<id>-whisper`.
    pub fn stem(&self) -> String {
        if self.whispered {
            format!("{}-whisper", self.line.id)
        } else {
            self.line.id.clone()
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// The script has a malformed line (1-based line number) or a duplicate id.
    Script(String),
    /// No such take in the script.
    UnknownTake,
    /// The take is shorter than `MIN_TAKE_SECS`.
    TooShort,
    /// The consent level is neither `private` nor `release`.
    BadConsent,
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Script(m) => write!(f, "reading script: {m}"),
            Error::UnknownTake => write!(f, "no such take in the reading script"),
            Error::TooShort => write!(f, "take shorter than {MIN_TAKE_SECS} s"),
            Error::BadConsent => write!(f, "consent level must be private or release"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// Parses a reading script (`id<TAB>lang<TAB>category<TAB>whisper<TAB>text`,
/// `#` comments and blank lines ignored), with the same checks as the
/// manifest builder.
pub fn parse(script: &str) -> Result<Vec<Line>, Error> {
    let mut lines: Vec<Line> = Vec::new();
    for (n, raw) in script.lines().enumerate() {
        if raw.trim().is_empty() || raw.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = raw.split('\t').collect();
        let ok = parts.len() == 5 && matches!(parts[1], "hu" | "en") && matches!(parts[3], "yes" | "no");
        if !ok || parts[0].is_empty() || parts[4].trim().is_empty() {
            return Err(Error::Script(format!("line {}: expected id, lang (hu|en), category, whisper (yes|no), text", n + 1)));
        }
        if lines.iter().any(|l| l.id == parts[0]) {
            return Err(Error::Script(format!("line {}: duplicate id {}", n + 1, parts[0])));
        }
        lines.push(Line {
            id: parts[0].to_string(),
            lang: parts[1].to_string(),
            category: parts[2].to_string(),
            whisper: parts[3] == "yes",
            text: parts[4].trim().to_string(),
        });
    }
    Ok(lines)
}

/// The takes in recording order: every line normally, then the whisper lines again whispered.
pub fn plan(lines: &[Line]) -> Vec<Take> {
    let normal = lines.iter().map(|l| Take { line: l.clone(), whispered: false });
    let whispered = lines.iter().filter(|l| l.whisper).map(|l| Take { line: l.clone(), whispered: true });
    normal.chain(whispered).collect()
}

/// The built-in script's takes.
pub fn builtin_plan() -> Vec<Take> {
    plan(&parse(SCRIPT).expect("built-in reading script is valid"))
}

/// The folder for one speaker under `root`: the name is reduced to letters,
/// digits, `-` and `_` (accents dropped to keep paths plain), "speaker" if nothing is left.
pub fn speaker_dir(root: &Path, name: &str) -> PathBuf {
    let clean: String = name
        .trim()
        .chars()
        .filter_map(|c| match c {
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' => Some(c),
            ' ' => Some('-'),
            c => fold(c),
        })
        .collect();
    let clean = clean.trim_matches('-');
    root.join(if clean.is_empty() { "speaker" } else { clean })
}

fn fold(c: char) -> Option<char> {
    let base = match c {
        'á' | 'à' | 'â' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'ő' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ű' => 'u',
        'Á' | 'À' | 'Â' | 'Ä' => 'A',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Ő' => 'O',
        'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ű' => 'U',
        _ => return None,
    };
    Some(base)
}

/// Whether a take already has a recording in `dir`.
pub fn is_recorded(dir: &Path, take: &Take) -> bool {
    dir.join(format!("{}.wav", take.stem())).is_file()
}

/// Index of the first take in `takes` not yet recorded in `dir`, or `takes.len()` when all are.
pub fn next_take(dir: &Path, takes: &[Take]) -> usize {
    takes.iter().position(|t| !is_recorded(dir, t)).unwrap_or(takes.len())
}

/// Saves one take: `<stem>.wav` and its reference text `<stem>.txt`, replacing
/// an earlier take of the same line. The folder is created if needed.
pub fn save_take(dir: &Path, takes: &[Take], id: &str, whispered: bool, samples: &[f32]) -> Result<PathBuf, Error> {
    let take = takes.iter().find(|t| t.line.id == id && t.whispered == whispered).ok_or(Error::UnknownTake)?;
    if (samples.len() as f32) < MIN_TAKE_SECS * SAMPLE_RATE as f32 {
        return Err(Error::TooShort);
    }
    fs::create_dir_all(dir)?;
    let wav = dir.join(format!("{}.wav", take.stem()));
    crate::debug::write_wav(&wav, samples)?;
    fs::write(dir.join(format!("{}.txt", take.stem())), format!("{}\n", take.line.text))?;
    Ok(wav)
}

/// Writes `consent.txt`: one line, `<level> <date> <who>` (level `private` or `release`).
pub fn write_consent(dir: &Path, level: &str, date: &str, who: &str) -> Result<(), Error> {
    if !matches!(level, "private" | "release") {
        return Err(Error::BadConsent);
    }
    fs::create_dir_all(dir)?;
    let who = who.split_whitespace().collect::<Vec<_>>().join(" ");
    let date = date.split_whitespace().collect::<Vec<_>>().join("-");
    fs::write(dir.join("consent.txt"), format!("{level} {date} {who}").trim_end().to_string() + "\n")?;
    Ok(())
}

/// The consent level saved in `dir`, as the manifest builder reads it:
/// `Some("release")` only when the first word says so, `Some("private")` for any
/// other file, `None` when there is no file yet.
pub fn consent(dir: &Path) -> Option<&'static str> {
    let text = fs::read_to_string(dir.join("consent.txt")).ok()?;
    let first = text.split_whitespace().next().unwrap_or("").to_lowercase();
    Some(if first == "release" { "release" } else { "private" })
}

/// The plan as JSON for the app: `{"consent": null|"private"|"release", "next": n,
/// "takes": [{"id", "lang", "category", "whispered", "text", "done"}]}`.
pub fn status_json(dir: &Path, takes: &[Take]) -> String {
    let mut out = String::from("{\"consent\":");
    match consent(dir) {
        Some(level) => json_str(&mut out, level),
        None => out.push_str("null"),
    }
    out.push_str(&format!(",\"next\":{},\"takes\":[", next_take(dir, takes)));
    for (i, t) in takes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        json_str(&mut out, &t.line.id);
        out.push_str(",\"lang\":");
        json_str(&mut out, &t.line.lang);
        out.push_str(",\"category\":");
        json_str(&mut out, &t.line.category);
        out.push_str(&format!(",\"whispered\":{},\"text\":", t.whispered));
        json_str(&mut out, &t.line.text);
        out.push_str(&format!(",\"done\":{}}}", is_recorded(dir, t)));
    }
    out.push_str("]}");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pusmogo-testset-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    const SMALL: &str = "# comment\nts001\thu\teveryday\tyes\tHolnap reggel.\n\nts002\ten\teveryday-en\tno\tSee you.\n";

    #[test]
    fn builtin_script_parses_with_twenty_whisper_lines() {
        let lines = parse(SCRIPT).unwrap();
        assert_eq!(lines.len(), 130);
        assert_eq!(lines.iter().filter(|l| l.whisper).count(), 20);
        assert_eq!(builtin_plan().len(), 150);
    }

    #[test]
    fn malformed_and_duplicate_lines_are_rejected() {
        assert!(matches!(parse("a\thu\tx\tmaybe\ttext"), Err(Error::Script(_))));
        assert!(matches!(parse("a\tde\tx\tno\ttext"), Err(Error::Script(_))));
        assert!(matches!(parse("a\thu\tx\tno\t "), Err(Error::Script(_))));
        assert!(matches!(parse("a\thu\tx\tno\tegy\na\thu\tx\tno\tketto"), Err(Error::Script(_))));
    }

    #[test]
    fn plan_reads_normal_first_then_whispers() {
        let takes = plan(&parse(SMALL).unwrap());
        let stems: Vec<String> = takes.iter().map(Take::stem).collect();
        assert_eq!(stems, vec!["ts001", "ts002", "ts001-whisper"]);
    }

    #[test]
    fn speaker_names_become_plain_folder_names() {
        let root = Path::new("/r");
        assert_eq!(speaker_dir(root, "Kovács Éva"), root.join("Kovacs-Eva"));
        assert_eq!(speaker_dir(root, "../../etc"), root.join("etc"));
        assert_eq!(speaker_dir(root, "  "), root.join("speaker"));
        assert_eq!(speaker_dir(root, "s_01"), root.join("s_01"));
    }

    #[test]
    fn saving_takes_advances_the_next_take() {
        let dir = scratch("save");
        let takes = plan(&parse(SMALL).unwrap());
        assert_eq!(next_take(&dir, &takes), 0);
        let wav = save_take(&dir, &takes, "ts001", false, &[0.1; 8000]).unwrap();
        assert_eq!(wav, dir.join("ts001.wav"));
        assert_eq!(fs::read_to_string(dir.join("ts001.txt")).unwrap(), "Holnap reggel.\n");
        assert_eq!(next_take(&dir, &takes), 1);
        save_take(&dir, &takes, "ts002", false, &[0.1; 8000]).unwrap();
        save_take(&dir, &takes, "ts001", true, &[0.1; 8000]).unwrap();
        assert!(dir.join("ts001-whisper.wav").is_file());
        assert_eq!(next_take(&dir, &takes), 3);
        // 44-byte header + 2 bytes per sample.
        assert_eq!(fs::metadata(&wav).unwrap().len(), 44 + 16_000);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn short_or_unknown_takes_are_refused() {
        let dir = scratch("refuse");
        let takes = plan(&parse(SMALL).unwrap());
        assert!(matches!(save_take(&dir, &takes, "ts001", false, &[0.1; 4000]), Err(Error::TooShort)));
        assert!(matches!(save_take(&dir, &takes, "ts002", true, &[0.1; 8000]), Err(Error::UnknownTake)));
        assert!(matches!(save_take(&dir, &takes, "nope", false, &[0.1; 8000]), Err(Error::UnknownTake)));
        assert!(!dir.exists());
    }

    #[test]
    fn consent_round_trips_like_the_manifest_builder() {
        let dir = scratch("consent");
        assert_eq!(consent(&dir), None);
        assert!(matches!(write_consent(&dir, "maybe", "2026-11-09", "EK"), Err(Error::BadConsent)));
        write_consent(&dir, "release", "2026-11-09", "  E  K ").unwrap();
        assert_eq!(fs::read_to_string(dir.join("consent.txt")).unwrap(), "release 2026-11-09 E K\n");
        assert_eq!(consent(&dir), Some("release"));
        write_consent(&dir, "private", "2026-11-09", "").unwrap();
        assert_eq!(fs::read_to_string(dir.join("consent.txt")).unwrap(), "private 2026-11-09\n");
        assert_eq!(consent(&dir), Some("private"));
        fs::write(dir.join("consent.txt"), "igen\n").unwrap();
        assert_eq!(consent(&dir), Some("private"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn status_json_lists_takes_and_progress() {
        let dir = scratch("json");
        let takes = plan(&parse(SMALL).unwrap());
        save_take(&dir, &takes, "ts001", false, &[0.1; 8000]).unwrap();
        let json = status_json(&dir, &takes);
        assert!(json.starts_with("{\"consent\":null,\"next\":1,\"takes\":[{\"id\":\"ts001\""), "{json}");
        assert!(json.contains("\"whispered\":false,\"text\":\"Holnap reggel.\",\"done\":true}"), "{json}");
        assert!(json.ends_with("{\"id\":\"ts001\",\"lang\":\"hu\",\"category\":\"everyday\",\"whispered\":true,\"text\":\"Holnap reggel.\",\"done\":false}]}"), "{json}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
