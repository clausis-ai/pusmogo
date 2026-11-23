//! Guard against Whisper's text on silence.
//!
//! Whisper writes plausible text ("Köszönöm.", "NAMASTE", "Thank you.") when it
//! hears silence, room noise or a hum. In a dictation app that text would be
//! typed into the user's document when they release the key without speaking.
//!
//! Two checks, both pure functions so they can be unit-tested without a model:
//! - [`VoiceActivity`]: a frame-energy detector. Speech has large loudness swings
//!   between syllables and pauses; silence, steady noise and hum do not. A frame
//!   counts as voiced when it is clearly louder than the clip's own noise floor,
//!   so quiet microphones still pass while loud steady noise does not.
//! - [`is_known_hallucination`]: phrases Whisper is known to produce on non-speech.
//!   A segment is dropped only when it is one of these *and* its time span holds
//!   almost no voiced audio, so a user who really says "köszönöm" keeps it.
//! - [`drop_segment`]: the decision per segment. Room sounds with loudness swings
//!   (typing, music, chatter) pass the energy check, and Whisper then writes
//!   "Köszönöm." over them with low confidence; real speech of the same words is
//!   decoded with high confidence (`docs/bench/noise-01.md`). Over longer noise
//!   it also writes other words with very low confidence (`noise-02.md`).

/// 30 ms frames at 16 kHz.
pub const FRAME: usize = 480;
/// Seconds per frame.
const FRAME_SECS: f32 = FRAME as f32 / crate::SAMPLE_RATE as f32;
/// A voiced frame is at least this many dB above the noise floor...
const MARGIN_DB: f32 = 15.0;
/// ...and above this absolute level, so digital silence and dither never count.
const MIN_DB: f32 = -70.0;
/// Speech with no pauses has a smaller swing (about 13 dB in fluent TTS), so a
/// loud frame also counts when it is this far above the floor...
const LOUD_MARGIN_DB: f32 = 6.0;
/// ...and louder than this. Steady noise and hum vary by under 1 dB per frame.
const LOUD_DB: f32 = -40.0;
/// Below this much voiced audio the clip is treated as having no speech.
/// Shorter than any real word, longer than a key click.
pub const MIN_SPEECH_SECS: f32 = 0.12;

/// Per-frame voiced flags for one clip.
pub struct VoiceActivity {
    voiced: Vec<bool>,
}

impl VoiceActivity {
    pub fn analyze(samples: &[f32]) -> Self {
        let db: Vec<f32> = samples
            .chunks(FRAME)
            .filter(|f| f.len() == FRAME)
            .map(|f| {
                let rms = (f.iter().map(|s| s * s).sum::<f32>() / FRAME as f32).sqrt();
                20.0 * rms.max(1e-9).log10()
            })
            .collect();
        if db.is_empty() {
            return Self { voiced: Vec::new() };
        }
        let mut sorted = db.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let floor = sorted[(sorted.len() - 1) / 10];
        let quiet = (floor + MARGIN_DB).max(MIN_DB);
        let loud = (floor + LOUD_MARGIN_DB).max(LOUD_DB);
        Self { voiced: db.iter().map(|&d| d > quiet || d > loud).collect() }
    }

    /// One flag per 30 ms frame ([`FRAME`] samples): true when voiced.
    pub fn frames(&self) -> &[bool] {
        &self.voiced
    }

    /// Total voiced time in seconds.
    pub fn voiced_secs(&self) -> f32 {
        self.voiced.iter().filter(|&&v| v).count() as f32 * FRAME_SECS
    }

    /// Voiced time in seconds between `start` and `end` (seconds from clip start).
    pub fn voiced_secs_between(&self, start: f32, end: f32) -> f32 {
        let a = ((start.max(0.0) / FRAME_SECS) as usize).min(self.voiced.len());
        let b = ((end.max(0.0) / FRAME_SECS).ceil() as usize).clamp(a, self.voiced.len());
        self.voiced[a..b].iter().filter(|&&v| v).count() as f32 * FRAME_SECS
    }

    pub fn has_speech(&self) -> bool {
        self.voiced_secs() >= MIN_SPEECH_SECS
    }
}

/// Phrases Whisper emits on silence or noise, compared after lowercasing and
/// stripping punctuation. Keep this list to whole-utterance outputs seen in
/// practice; a phrase here is only dropped when the audio under it is silent.
const HALLUCINATIONS: &[&str] = &[
    "namaste",
    "köszönöm",
    "köszönöm szépen",
    "köszönöm a figyelmet",
    "köszönöm hogy megnézted",
    "köszönöm szépen a figyelmet",
    "köszönjük",
    "köszönjük szépen",
    "köszönjük a figyelmet",
    "feliratozta",
    "tisztelt képviselőtársaim",
    "elnök úr tisztelt képviselőtársaim",
    "thank you",
    "thanks for watching",
    "thank you for watching",
    "thank you very much",
    "you",
    "bye",
];

/// Subtitle credits from Whisper's training data, compared like
/// [`HALLUCINATIONS`]. large-v3 writes them over music with high confidence
/// (0.86-0.91) and over seconds of voiced audio, so neither check above
/// catches them (`docs/bench/bakeoff-05.md`). Nobody dictates a credit line,
/// so a segment that is exactly one of these is always dropped.
const SUBTITLE_CREDITS: &[&str] = &[
    "feliratot készítette amaraorg közösségével",
    "feliratok az amaraorg közösségétől",
    "subtitles by the amaraorg community",
];

/// True when `text` is only a subtitle credit.
pub fn is_subtitle_credit(text: &str) -> bool {
    SUBTITLE_CREDITS.contains(&normalize(text).as_str())
}

/// Openings of the longer sentences Whisper invents on noise ("Köszönöm, hogy
/// megtaláltad!"). Only dropped together with low decoding confidence.
/// "Köszönjük!" over 4 s of typing or chatter measured 0.64-0.67, really
/// spoken 0.87-0.99 (`docs/bench/noise-05.md`).
const HALLUCINATION_OPENINGS: &[&str] = &["köszönöm", "köszönjük", "thank you", "thanks for"];

/// Below this mean token probability a segment that looks like a hallucination
/// is dropped even over voiced audio. Measured: 0.60-0.69 for text invented over
/// typing, music and chatter; 0.93-0.98 for the same words really spoken.
pub const MIN_CONFIDENCE: f32 = 0.75;

/// A known phrase ("NAMASTE", "Thank you.") takes well under this much voiced
/// audio to say: at most 1.41 s measured, over chatter. Over 15 s of typing or
/// music Whisper wrote "NAMASTE" over 2.6-8.4 s of voiced audio.
pub const MAX_PHRASE_SECS: f32 = 2.0;
/// A known phrase spread over longer audio needs this much confidence. "NAMASTE"
/// over noise measured 0.76-0.79 with turbo and 0.862 over 2.25 s of music
/// with large-v3; confidence alone cannot tell it apart, since a spoken
/// "Thank you." over typing or chatter measured 0.78-0.85. Spoken known
/// phrases, slow speech and noise included, stayed at or under 2.04 s of
/// voiced audio, so this floor never applies to them (`bakeoff-05.md`).
pub const MIN_PHRASE_CONFIDENCE: f32 = 0.90;

/// Below this mean token probability any segment is dropped. Measured on 284
/// real speech segments (synth HU/mixed/EN, FLEURS HU, long set): lowest 0.48,
/// then 0.69. Text invented over 15 s of fan or chatter ("Re", "Közöszönöm...")
/// measured 0.00-0.32.
pub const MIN_ANY_CONFIDENCE: f32 = 0.40;

/// A short word or phrase (at most [`MAX_PHRASE_SECS`] of voiced audio) is
/// kept down to this probability. Single words spoken over fan, typing or
/// chatter decoded as near misses at 0.35-0.40 ("Levil" for "Levél", "RUKA"
/// for "Róka") and were dropped, so the user got nothing at all. Text invented
/// over short noise measured 0.00 ("Re"); over long noise 0.32 with 10.9 s of
/// voiced audio, which still meets the 0.40 floor (`docs/bench/noise-04.md`).
pub const MIN_SHORT_CONFIDENCE: f32 = 0.30;

fn normalize(text: &str) -> String {
    let norm: String = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    norm.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// True when `text` is only a known silence hallucination.
pub fn is_known_hallucination(text: &str) -> bool {
    let norm = normalize(text);
    !norm.is_empty() && HALLUCINATIONS.contains(&norm.as_str())
}

/// True when `text` is a known hallucination or starts like one.
fn looks_like_hallucination(text: &str) -> bool {
    let norm = normalize(text);
    is_known_hallucination(text)
        || HALLUCINATION_OPENINGS
            .iter()
            .any(|o| norm == *o || norm.starts_with(&format!("{o} ")))
}

/// Whether to drop one recognised segment. `voiced_secs` is the voiced audio
/// under the segment, `confidence` the mean probability of its text tokens.
/// Ordinary text is kept unless the model was almost guessing.
pub fn drop_segment(text: &str, voiced_secs: f32, confidence: f32) -> bool {
    let known = is_known_hallucination(text);
    let short = (MIN_SPEECH_SECS..=MAX_PHRASE_SECS).contains(&voiced_secs);
    let floor = if short { MIN_SHORT_CONFIDENCE } else { MIN_ANY_CONFIDENCE };
    confidence < floor
        || is_subtitle_credit(text)
        || (known && voiced_secs < MIN_SPEECH_SECS)
        || (known && voiced_secs > MAX_PHRASE_SECS && confidence < MIN_PHRASE_CONFIDENCE)
        || (looks_like_hallucination(text) && confidence < MIN_CONFIDENCE)
}

/// Whether a segment only repeats the Hungarian prompt label: every word is a
/// label word or the start of one ("Gyakor.", "Ez egy magyar nyelvű."), and
/// it holds a piece of "Gyakori szavak" or at least three label words. With a
/// prompt, Whisper sometimes echoes the label over short noisy clips instead
/// of guessing a word (7 of 528 short clips, all wrong anyway;
/// docs/bench/prompt-02.md). Only call it when a prompt was given.
pub fn echoes_prompt_label(text: &str) -> bool {
    let label = normalize(crate::cleanup::HU_PROMPT_LABEL);
    let label: Vec<&str> = label.split_whitespace().collect();
    let norm = normalize(text);
    let words: Vec<&str> = norm.split_whitespace().collect();
    let all_label = !words.is_empty()
        && words.iter().all(|w| w.chars().count() >= 2 && label.iter().any(|l| l.starts_with(w)));
    all_label && (words.len() >= 3 || words.iter().any(|w| w.starts_with("gyakor") || w.starts_with("szava")))
}

/// File name of the Silero speech detector (ggml format, about 0.9 MB), looked
/// up next to the Whisper model. Optional: without it every clip that passes
/// the energy check goes to Whisper.
pub const DETECTOR_MODEL_FILE: &str = "ggml-silero-v5.1.2.bin";
/// Level the detector's input is scaled to. The detector misses short words
/// below about -40 dBFS, while Whisper still hears them, so the clip is
/// normalised first (`docs/bench/noise-03.md`).
const DETECTOR_PEAK: f32 = 0.5;
/// Most gain applied, so digital silence stays silent.
const MAX_DETECTOR_GAIN: f32 = 1000.0;

/// The clip scaled for the speech detector. The level comes from the loudest
/// 30 ms frames, skipping the top 1 % so a single key click does not set it.
pub fn detector_input(samples: &[f32]) -> Vec<f32> {
    let mut peaks: Vec<f32> = samples.chunks(FRAME).map(|f| f.iter().fold(0.0f32, |m, s| m.max(s.abs()))).collect();
    if peaks.is_empty() {
        return Vec::new();
    }
    peaks.sort_by(|a, b| b.total_cmp(a));
    let level = peaks[peaks.len() / 100];
    let gain = if level > 0.0 { (DETECTOR_PEAK / level).min(MAX_DETECTOR_GAIN) } else { 1.0 };
    samples.iter().map(|s| (s * gain).clamp(-1.0, 1.0)).collect()
}

/// Below this many characters per voiced second a prompted result is
/// suspect. FLEURS Hungarian never went under 11.7 per segment (median 17);
/// a clip that says one sentence twice came back with it once at 8.0.
const MIN_CHARS_PER_VOICED_SEC: f32 = 9.0;
/// Too little voice to judge the density.
const MIN_DENSITY_SECS: f32 = 4.0;
/// The plain result must be this much longer to replace the prompted one.
const MIN_RECOVERED_RATIO: f32 = 1.5;

/// Whether a result decoded with a prompt looks like it skipped speech: far
/// fewer characters than the voiced audio should give. With a prompt Whisper
/// sometimes decodes a repeated sentence only once (docs/bench/repeat-01.md).
pub fn prompt_skipped_speech(text: &str, voiced_secs: f32) -> bool {
    voiced_secs >= MIN_DENSITY_SECS && (text.chars().count() as f32) < MIN_CHARS_PER_VOICED_SEC * voiced_secs
}

/// Whether the result decoded without the prompt recovered the skipped speech.
/// Otherwise the prompted text is kept, because it spells names better.
pub fn prefer_plain(prompted: &str, plain: &str) -> bool {
    plain.chars().count() as f32 >= MIN_RECOVERED_RATIO * prompted.chars().count() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_echo_is_caught() {
        for echo in ["Gyakor.", "Gyakor, Gyakor, Gyakor.", "Ez egy magyar nyelvű.", "Gyakori szavak:"] {
            assert!(echoes_prompt_label(echo), "{echo}");
        }
        for real in ["Ez egy.", "Igen.", "Ez egy jó nap.", "Gyakor, Ares.", "", "Magyar."] {
            assert!(!echoes_prompt_label(real), "{real}");
        }
    }

    #[test]
    fn sparse_prompted_text_is_suspect() {
        let once = "Holnap Szűts Ottó és Bencze Lila is eljön a Clausis megbeszélésére, Gyuriça Péter pedig online csatlakozik.";
        assert!(prompt_skipped_speech(once, 13.4));
        assert!(!prompt_skipped_speech(once, 6.1));
        // Too short to judge.
        assert!(!prompt_skipped_speech("Igen.", 1.0));
        assert!(!prompt_skipped_speech("", 3.0));
    }

    #[test]
    fn plain_result_must_recover_text() {
        let once = "Holnap jövök.";
        assert!(prefer_plain(once, "Holnap jövök. Holnap jövök."));
        assert!(!prefer_plain(once, "Holnap jövök, jó?"));
    }

    const RATE: usize = crate::SAMPLE_RATE as usize;

    /// Deterministic white noise at roughly `dbfs` RMS.
    fn noise(secs: f32, dbfs: f32) -> Vec<f32> {
        let amp = 10f32.powf(dbfs / 20.0) * 3f32.sqrt();
        let mut x: u32 = 12345;
        (0..(secs * RATE as f32) as usize)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((x >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * amp
            })
            .collect()
    }

    fn tone(secs: f32, dbfs: f32, hz: f32) -> Vec<f32> {
        let amp = 10f32.powf(dbfs / 20.0) * 2f32.sqrt();
        (0..(secs * RATE as f32) as usize)
            .map(|i| amp * (i as f32 * hz * std::f32::consts::TAU / RATE as f32).sin())
            .collect()
    }

    /// Syllable-like bursts: 150 ms loud, 100 ms quiet, over a noise floor.
    fn speechlike(secs: f32, floor_db: f32, peak_db: f32) -> Vec<f32> {
        let mut out = noise(secs, floor_db);
        let burst = tone(secs, peak_db, 220.0);
        for (i, s) in out.iter_mut().enumerate() {
            if i % (RATE / 4) < RATE * 3 / 20 {
                *s += burst[i];
            }
        }
        out
    }

    #[test]
    fn silence_and_steady_noise_have_no_speech() {
        assert!(!VoiceActivity::analyze(&vec![0.0; RATE]).has_speech());
        assert!(!VoiceActivity::analyze(&[]).has_speech());
        for db in [-60.0, -45.0, -35.0, -20.0] {
            assert!(!VoiceActivity::analyze(&noise(3.0, db)).has_speech(), "noise {db}");
        }
        assert!(!VoiceActivity::analyze(&tone(3.0, -40.0, 50.0)).has_speech(), "hum");
    }

    #[test]
    fn speech_is_detected_loud_and_quiet() {
        assert!(VoiceActivity::analyze(&speechlike(2.0, -60.0, -15.0)).has_speech());
        // A quiet microphone: speech peaks around -50 dBFS over a -85 dBFS floor.
        assert!(VoiceActivity::analyze(&speechlike(2.0, -85.0, -50.0)).has_speech());
        // Fluent speech with no pause: only about 10 dB between syllables and peaks.
        assert!(VoiceActivity::analyze(&speechlike(2.0, -25.0, -14.0)).has_speech());
        // Speech over a steady fan.
        assert!(VoiceActivity::analyze(&speechlike(2.0, -40.0, -18.0)).has_speech());
    }

    #[test]
    fn a_key_click_is_not_speech() {
        let mut clip = vec![0.0; RATE];
        for s in clip.iter_mut().take(FRAME * 2) {
            *s = 0.5;
        }
        assert!(!VoiceActivity::analyze(&clip).has_speech());
    }

    #[test]
    fn voiced_time_in_a_window() {
        let mut clip = speechlike(2.0, -60.0, -15.0);
        clip.extend(noise(1.0, -60.0));
        let va = VoiceActivity::analyze(&clip);
        assert!(va.voiced_secs_between(0.0, 2.0) > 0.5);
        assert_eq!(va.voiced_secs_between(2.1, 3.0), 0.0);
        assert_eq!(va.voiced_secs_between(5.0, 9.0), 0.0);
    }

    #[test]
    fn detector_input_is_normalised_and_ignores_a_click() {
        let quiet = speechlike(4.0, -80.0, -50.0);
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let scaled = detector_input(&quiet);
        assert!((peak(&scaled) - DETECTOR_PEAK).abs() < 0.01);
        // One loud click among quiet speech does not set the level.
        let mut clicked = quiet.clone();
        clicked[1000] = 0.9;
        let scaled = detector_input(&clicked);
        assert!(scaled[2000..].iter().fold(0.0f32, |m, s| m.max(s.abs())) > 0.4);
        // Digital silence stays silent.
        assert!(detector_input(&vec![0.0; 16_000]).iter().all(|&s| s == 0.0));
        assert!(detector_input(&[]).is_empty());
    }

    #[test]
    fn known_hallucinations() {
        assert!(is_known_hallucination("NAMASTE"));
        assert!(is_known_hallucination(" Köszönöm. "));
        assert!(is_known_hallucination("Thank you!"));
        assert!(!is_known_hallucination(""));
        assert!(!is_known_hallucination("Köszönöm a gyors választ."));
    }

    #[test]
    fn segments_dropped_over_silence_or_with_low_confidence() {
        // Over silence: dropped whatever the confidence.
        assert!(drop_segment("Köszönöm.", 0.0, 0.99));
        // Over typing or music: the energy check passes, confidence is low.
        assert!(drop_segment("Köszönöm.", 2.9, 0.60));
        assert!(drop_segment("Köszönöm, hogy megtaláltad!", 2.3, 0.69));
        assert!(drop_segment("Thank you for watching.", 1.0, 0.5));
        // Really spoken: high confidence, kept.
        assert!(!drop_segment("Köszönöm.", 0.5, 0.97));
        assert!(!drop_segment("Köszönöm, hogy megtaláltad.", 1.7, 0.98));
        assert!(!drop_segment("Thank you.", 0.66, 0.93));
        // Ordinary text is kept when the model was unsure...
        assert!(!drop_segment("Kérlek, hűd el a jelentést.", 3.5, 0.48));
        assert!(!drop_segment("Köszönetet mondok.", 1.0, 0.69));
        assert!(!drop_segment("Köszönöm, hogy szóltál.", 3.0, 0.77));
        // ...but not when it was almost guessing (15 s of fan or chatter).
        assert!(drop_segment("re", 0.39, 0.0));
        assert!(drop_segment("Közöszönöm...", 10.9, 0.32));
    }

    #[test]
    fn short_near_misses_in_noise_are_kept() {
        // One word over fan, typing or chatter: wrong spelling, but typed.
        assert!(!drop_segment(" Levil", 0.63, 0.347));
        assert!(!drop_segment(" RUKA", 0.9, 0.365));
        // Still dropped: guessing, too little voice, or a long span.
        assert!(drop_segment(" Re", 0.39, 0.0));
        assert!(drop_segment(" Levil", 0.05, 0.35));
        assert!(drop_segment(" Hétúra éres!", 2.5, 0.35));
    }

    #[test]
    fn known_phrase_needs_high_confidence_over_long_noise() {
        // 15 s of typing or music: "NAMASTE" at 0.76-0.79 over seconds of voiced audio.
        assert!(drop_segment(" NAMASTE", 2.6, 0.76));
        assert!(drop_segment(" NAMASTE", 8.4, 0.79));
        // large-v3 over 4 s of music (bakeoff-05).
        assert!(drop_segment(" NAMASTE", 2.25, 0.862));
        // Really spoken, over typing or chatter: short, so kept at the same confidence.
        assert!(!drop_segment(" Thank you.", 0.66, 0.78));
        assert!(!drop_segment(" Bye.", 0.33, 0.775));
        assert!(!drop_segment(" Köszönöm szépen!", 1.41, 0.80));
        // A sentence that only starts like a phrase keeps the lower bar.
        assert!(!drop_segment("Köszönöm, hogy megtaláltad.", 1.7, 0.80));
    }

    #[test]
    fn subtitle_credits_are_always_dropped() {
        // large-v3 over 4-15 s of music, detector on and off (bakeoff-05).
        assert!(drop_segment(" Feliratot készítette Amara.org közösségével", 8.4, 0.855));
        assert!(drop_segment(" Feliratok az Amara.org közösségétől", 0.39, 0.905));
        assert!(drop_segment(" Feliratok az Amara.org közösségétől", 2.88, 0.904));
    }

    #[test]
    fn parliament_openings_over_noise_are_dropped_but_spoken_are_kept() {
        // The Hungarian fine-tune learned these from parliament speeches and
        // writes them over music and long noise (docs/bench/train-01.md).
        assert!(drop_segment(" Tisztelt képviselőtársaim!", 2.25, 0.808));
        assert!(drop_segment(" Elnök Úr, tisztelt Képviselőtársaim!", 8.4, 0.794));
        // Spoken (TTS): high confidence, kept; and other sentences are untouched.
        assert!(!drop_segment(" Tisztelt képviselőtársaim!", 1.8, 0.95));
        assert!(!drop_segment(" Elnök úr, tisztelt képviselőtársaim!", 2.49, 0.953));
        assert!(!drop_segment(" Tisztelt elnök úr!", 1.17, 0.954));
        assert!(drop_segment("Subtitles by the Amara.org community", 3.0, 0.95));
        // Only the whole credit: a sentence that mentions the site is kept.
        assert!(!drop_segment("Nézd meg az amara.org oldalt.", 1.5, 0.9));
        assert!(!drop_segment("Feliratot készítettem a videóhoz.", 1.8, 0.9));
    }

    #[test]
    fn plural_thanks_over_noise_is_dropped_but_spoken_is_kept() {
        // Detector off: 4 s of typing and of chatter (docs/bench/noise-05.md).
        assert!(drop_segment(" Köszönjük!", 0.69, 0.674));
        assert!(drop_segment(" Köszönjük!", 2.88, 0.639));
        // Spoken clean or over room sounds.
        assert!(!drop_segment(" Köszönjük!", 0.48, 0.874));
        assert!(!drop_segment(" Köszönjük szépen!", 0.69, 0.971));
        assert!(!drop_segment(" Köszönjük, hogy eljöttetek!", 1.59, 0.945));
        assert!(!drop_segment(" Köszönjük a segítséget!", 1.11, 0.978));
    }
}
