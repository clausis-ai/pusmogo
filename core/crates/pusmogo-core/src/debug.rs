//! Opt-in local record of recent dictations, for replaying real failures.
//!
//! When the app turns it on (`pusmogo_set_debug_dir`), every finished
//! dictation leaves two files in the chosen folder: the exact 16 kHz audio the
//! engine received (`<stamp>.wav`) and what it made of it (`<stamp>.json`: raw
//! engine text, cleaned text, language, prompt and context hint words). Only
//! the newest `KEEP` dictations are kept. Nothing leaves the Mac; it is off by
//! default.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::SAMPLE_RATE;

/// How many dictations stay on disk.
pub const KEEP: usize = 20;

/// What the engine made of one dictation.
pub struct Record<'a> {
    pub lang: &'a str,
    pub raw: &'a str,
    pub cleaned: &'a str,
    pub prompt: &'a str,
    pub hints: &'a [String],
}

/// Writes `samples` (16 kHz mono f32) as 16-bit PCM WAV.
pub fn write_wav(path: &Path, samples: &[f32]) -> io::Result<()> {
    let data_len = u32::try_from(samples.len() * 2).map_err(|_| io::Error::other("audio too long"))?;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    fs::File::create(path)?.write_all(&out)
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The record as one JSON object (the audio length is given in seconds).
pub fn record_json(r: &Record, audio_secs: f32) -> String {
    let hints = r.hints.iter().map(|h| json_string(h)).collect::<Vec<_>>().join(", ");
    format!(
        "{{\n  \"lang\": {},\n  \"audio_secs\": {:.2},\n  \"raw\": {},\n  \"cleaned\": {},\n  \"prompt\": {},\n  \"hints\": [{}]\n}}\n",
        json_string(r.lang),
        audio_secs,
        json_string(r.raw),
        json_string(r.cleaned),
        json_string(r.prompt),
        hints
    )
}

/// Saves one dictation in `dir` (created if missing) and deletes the oldest
/// beyond `KEEP`. Returns the WAV path.
pub fn keep(dir: &Path, samples: &[f32], record: &Record) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    // Millisecond stamps sort in time order and never collide in practice
    // (a dictation takes far longer than 1 ms).
    let stem = format!("{:013}", now.as_millis());
    let wav = dir.join(format!("{stem}.wav"));
    write_wav(&wav, samples)?;
    fs::write(dir.join(format!("{stem}.json")), record_json(record, samples.len() as f32 / SAMPLE_RATE as f32))?;
    prune(dir, KEEP)?;
    Ok(wav)
}

/// Keeps the newest `keep` dictations (by file stem) in `dir`.
pub fn prune(dir: &Path, keep: usize) -> io::Result<()> {
    let mut stems: Vec<String> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".wav")?;
            stem.bytes().all(|b| b.is_ascii_digit()).then(|| stem.to_string())
        })
        .collect();
    stems.sort();
    let excess = stems.len().saturating_sub(keep);
    for stem in &stems[..excess] {
        let _ = fs::remove_file(dir.join(format!("{stem}.wav")));
        let _ = fs::remove_file(dir.join(format!("{stem}.json")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pusmogo-debug-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn record<'a>(hints: &'a [String]) -> Record<'a> {
        Record { lang: "hu", raw: "Mi a López?", cleaned: "Mi a López?", prompt: "Clausis, \"Pusmogo\"", hints }
    }

    #[test]
    fn wav_header_and_samples() {
        let dir = scratch("wav");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.wav");
        write_wav(&path, &[0.0, 1.0, -1.0, 2.0]).unwrap();
        let b = fs::read(&path).unwrap();
        assert_eq!(b.len(), 44 + 8);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(u32::from_le_bytes(b[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(b[40..44].try_into().unwrap()), 8);
        let s: Vec<i16> = b[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(s, vec![0, i16::MAX, -i16::MAX, i16::MAX]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn json_escapes_quotes_and_newlines() {
        let hints = vec!["Kovács".to_string(), "a\"b".to_string()];
        let mut r = record(&hints);
        r.raw = "első\nmásodik";
        let j = record_json(&r, 1.5);
        assert!(j.contains("\"raw\": \"első\\nmásodik\""));
        assert!(j.contains("\"prompt\": \"Clausis, \\\"Pusmogo\\\"\""));
        assert!(j.contains("\"hints\": [\"Kovács\", \"a\\\"b\"]"));
        assert!(j.contains("\"audio_secs\": 1.50"));
    }

    #[test]
    fn keeps_only_the_newest() {
        let dir = scratch("prune");
        fs::create_dir_all(&dir).unwrap();
        for i in 0..5 {
            let stem = format!("{:013}", 1000 + i);
            fs::write(dir.join(format!("{stem}.wav")), b"x").unwrap();
            fs::write(dir.join(format!("{stem}.json")), b"{}").unwrap();
        }
        fs::write(dir.join("notes.txt"), b"mine").unwrap();
        prune(&dir, 2).unwrap();
        let mut left: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, vec!["0000000001003.json", "0000000001003.wav", "0000000001004.json", "0000000001004.wav", "notes.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keep_writes_a_pair() {
        let dir = scratch("keep");
        let hints = vec![];
        let wav = keep(&dir, &[0.1; 16_000], &record(&hints)).unwrap();
        assert!(wav.exists());
        let json = fs::read_to_string(wav.with_extension("json")).unwrap();
        assert!(json.contains("\"audio_secs\": 1.00"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
