//! Streaming dictation: transcribe while the user is still speaking.
//!
//! Whisper's cost grows with the audio it has to decode, so a 60 s dictation
//! transcribed after the key is released keeps the user waiting several times
//! longer than a 5 s one. A [`Session`] collects audio as it arrives and, each
//! time [`Session::step`] is called (the app calls it about once a second while
//! recording), transcribes a finished chunk in the background: audio up to a
//! pause in speech once at least [`MIN_CHUNK_SECS`] is waiting. When the key is
//! released, [`Session::finish`] only has the short tail left to transcribe.
//!
//! Chunks are cut in pauses, never inside a word, so each chunk is a complete
//! utterance for the engine. With no pause for [`MAX_CHUNK_SECS`], the chunk is
//! cut at its quietest frame.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::engine::{Engine, EngineError, Language};
use crate::vad::{VoiceActivity, FRAME};

/// Below this much waiting audio no chunk is cut: short chunks give the engine
/// too little context and cost a full model pass each.
pub const MIN_CHUNK_SECS: f32 = 8.0;
/// With no pause, a chunk is forced at this length (Whisper's window is 30 s).
pub const MAX_CHUNK_SECS: f32 = 24.0;
/// The shortest silence that counts as a pause between words or sentences.
pub const MIN_PAUSE_SECS: f32 = 0.3;

/// The final pass covers as many of the last chunks as fit in this much audio,
/// inside Whisper's 30 s window: one window costs about the same however much of
/// it is filled, and a tail cut from its sentence is often misheard alone. So a
/// dictation under 29 s is transcribed exactly as without streaming.
pub const FINAL_PASS_SECS: f32 = 29.0;

/// A caption preview of the audio not yet in a chunk is made once this much is
/// waiting. Without it the first words would appear only after the first
/// chunk, 8 s or more in. Short dictations (the common case) get none: they
/// barely need a caption, and a key release has to wait for a running pass.
pub const PREVIEW_SECS: f32 = 4.0;
/// After a preview, the next one waits for this much more audio, so the GPU
/// is busy (and a release likely to wait) for a smaller share of the time.
pub const PREVIEW_EVERY_SECS: f32 = 3.0;
/// No preview starts unless there was speech in this last stretch of audio: a
/// silence at the end often means the key is about to be released, and a
/// release has to wait for a pass already running on the GPU.
pub const PREVIEW_VOICED_TAIL_SECS: f32 = 0.5;

const RATE: usize = crate::SAMPLE_RATE as usize;

/// Audio no pass can need again is dropped once this much of it has piled up,
/// so the front of the buffer is moved rarely (a long hands-free dictation
/// would otherwise keep ~230 MB of samples per hour).
const DROP_SECS: f32 = 10.0;

/// One dictation. Not thread-safe: push and step from one thread or queue.
pub struct Session {
    lang: Language,
    /// The audio from absolute sample `offset` on; everything before it was
    /// transcribed in chunks that [`Session::finish`] can no longer redo.
    samples: Vec<f32>,
    offset: usize,
    /// Keep every sample (the opt-in dictation record saves the whole audio).
    pub keep_all: bool,
    /// Samples before this absolute index are transcribed.
    committed: usize,
    parts: Vec<String>,
    /// Start sample of each part.
    starts: Vec<usize>,
    /// Caption-only text of `samples[committed..preview_end]`. Never part of
    /// the result: [`Session::finish`] transcribes that audio again.
    preview: String,
    preview_end: usize,
    /// Whether the caller wants caption previews (the C ABI step checks it).
    pub previews: bool,
}

impl Session {
    pub fn new(lang: Language) -> Self {
        Self {
            lang,
            samples: Vec::new(),
            offset: 0,
            keep_all: false,
            committed: 0,
            parts: Vec::new(),
            starts: Vec::new(),
            preview: String::new(),
            preview_end: 0,
            previews: false,
        }
    }

    /// The audio still kept (16 kHz mono): all of it with [`Session::keep_all`],
    /// else at least everything a later pass can read, which is the whole
    /// dictation until it is longer than [`FINAL_PASS_SECS`] plus a chunk.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The dictation language this session was started with.
    pub fn lang(&self) -> Language {
        self.lang
    }

    /// Appends 16 kHz mono samples.
    pub fn push(&mut self, samples: &[f32]) {
        self.samples.extend_from_slice(samples);
    }

    /// Absolute index one past the last sample received.
    fn len(&self) -> usize {
        self.offset + self.samples.len()
    }

    /// Kept audio from absolute index `from` to `to`.
    fn audio(&self, from: usize, to: usize) -> &[f32] {
        &self.samples[from - self.offset..to - self.offset]
    }

    /// Seconds of audio received so far.
    pub fn total_secs(&self) -> f32 {
        self.len() as f32 / RATE as f32
    }

    /// Seconds of audio not yet transcribed.
    pub fn pending_secs(&self) -> f32 {
        (self.len() - self.committed) as f32 / RATE as f32
    }

    /// Seconds of audio currently held in memory.
    pub fn kept_secs(&self) -> f32 {
        self.samples.len() as f32 / RATE as f32
    }

    /// Number of chunks transcribed so far.
    pub fn chunks_done(&self) -> usize {
        self.parts.len()
    }

    /// Where the next chunk should end (absolute sample index), if one is ready.
    pub fn next_split(&self) -> Option<usize> {
        split_point(self.audio(self.committed, self.len())).map(|i| self.committed + i)
    }

    /// Transcribes one finished chunk if one is ready. Returns whether it did.
    pub fn step(&mut self, engine: &mut dyn Engine, prompt: &str) -> Result<bool, EngineError> {
        let Some(end) = self.next_split() else { return Ok(false) };
        let text = engine.transcribe(self.audio(self.committed, end), self.lang, prompt)?;
        self.commit(end, text);
        Ok(true)
    }

    /// Like [`Session::step`], but a key release (`stop`) may abandon the
    /// chunk pass midway when [`Session::finish`] would redo that chunk anyway:
    /// its start lies within [`FINAL_PASS_SECS`] of the end of the audio. Then
    /// nothing is lost and the release does not wait for a pass whose text
    /// would be thrown away (`docs/bench/stream-04.md`). If more audio
    /// arrives before the release than one final pass holds, `finish` cuts
    /// that chunk again. A chunk further back always runs to the end.
    pub fn step_until(&mut self, engine: &mut dyn Engine, prompt: &str, stop: &AtomicBool) -> Result<bool, EngineError> {
        let Some(end) = self.next_split() else { return Ok(false) };
        let chunk = self.audio(self.committed, end);
        let redone = self.len() - self.committed <= (FINAL_PASS_SECS * RATE as f32) as usize;
        let text = if redone {
            match engine.transcribe_abortable(chunk, self.lang, prompt, stop)? {
                Some(text) => text,
                None => return Ok(false),
            }
        } else {
            engine.transcribe(chunk, self.lang, prompt)?
        };
        self.commit(end, text);
        Ok(true)
    }

    /// Transcribes the waiting audio for the live caption only, when at least
    /// [`PREVIEW_SECS`] of audio is waiting after the last chunk and
    /// [`PREVIEW_EVERY_SECS`] arrived since the last preview. Call it when [`Session::step`] had no chunk to cut. The
    /// pass gives up as soon as `stop` is set (the key was released, so the
    /// final pass must not wait behind it). Returns whether the caption changed.
    pub fn preview(&mut self, engine: &mut dyn Engine, prompt: &str, stop: &AtomicBool) -> Result<bool, EngineError> {
        let len = self.len();
        let (waiting, since) = (len - self.committed, len - self.preview_end.max(self.committed));
        let due = if self.preview_end > self.committed { since as f32 >= PREVIEW_EVERY_SECS * RATE as f32 } else { true };
        if (waiting as f32) < PREVIEW_SECS * RATE as f32 || !due || stop.load(Ordering::Relaxed) || !self.speaking() {
            return Ok(false);
        }
        let Some(text) = engine.transcribe_until(self.audio(self.committed, len), self.lang, prompt, stop)? else {
            return Ok(false);
        };
        self.preview = until_repeat(text.trim()).to_string();
        self.preview_end = len;
        Ok(true)
    }

    /// Whether the last [`PREVIEW_VOICED_TAIL_SECS`] of the waiting audio hold
    /// speech, judged against the noise floor of all the waiting audio.
    fn speaking(&self) -> bool {
        let activity = VoiceActivity::analyze(self.audio(self.committed, self.len()));
        let tail = (PREVIEW_VOICED_TAIL_SECS * RATE as f32 / FRAME as f32).ceil() as usize;
        activity.frames().iter().rev().take(tail).any(|&v| v)
    }

    /// Raw text for the live caption: the chunks so far and the latest preview.
    pub fn caption(&self) -> String {
        let mut text = self.text();
        if self.preview_end > self.committed && !self.preview.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&self.preview);
        }
        text
    }

    /// Transcribes the remaining audio and returns the raw text of the whole
    /// dictation (cleanup is the caller's job, once, on the joined text).
    pub fn finish(&mut self, engine: &mut dyn Engine, prompt: &str) -> Result<String, EngineError> {
        let len = self.len();
        let max = (FINAL_PASS_SECS * RATE as f32) as usize;
        if self.committed < len {
            // Redo the earliest chunks that still fit in one pass with the tail.
            if let Some(i) = self.starts.iter().position(|&start| len - start <= max) {
                self.committed = self.starts[i];
                self.parts.truncate(i);
                self.starts.truncate(i);
            }
        }
        // More than one pass waiting (steps fell behind, or a release abandoned
        // a chunk pass): cut chunks first, so every engine call is one window.
        while self.pending_secs() > FINAL_PASS_SECS && self.step(engine, prompt)? {}
        if self.committed < len {
            let text = engine.transcribe(self.audio(self.committed, len), self.lang, prompt)?;
            self.commit(len, text);
        }
        Ok(self.text())
    }

    /// Raw text of the chunks transcribed so far.
    pub fn text(&self) -> String {
        self.parts.iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(" ")
    }

    fn commit(&mut self, end: usize, text: String) {
        self.starts.push(self.committed);
        self.committed = end;
        self.parts.push(text.trim().to_string());
        self.drop_done_audio();
    }

    /// Drops audio no pass can read again. [`Session::finish`] rewinds at most
    /// to the start of a chunk within [`FINAL_PASS_SECS`] of the end, and the
    /// end only moves on, so a chunk start further back than that is never
    /// read again; nor is anything before it. Steps, previews and the speech
    /// check only read from `committed` on. So the output is the same as with
    /// every sample kept.
    fn drop_done_audio(&mut self) {
        if self.keep_all {
            return;
        }
        let (len, max) = (self.len(), (FINAL_PASS_SECS * RATE as f32) as usize);
        let keep_from = self.starts.iter().copied().find(|&start| len - start <= max).unwrap_or(self.committed);
        if keep_from - self.offset >= (DROP_SECS * RATE as f32) as usize {
            self.samples.drain(..keep_from - self.offset);
            self.offset = keep_from;
        }
    }
}

/// Preview text up to the first run of four words that already appeared in it.
/// Previews use a short encoder window (`engine::preview_audio_ctx`), where
/// Whisper sometimes says a sentence again past the end of the speech; the
/// caption would show it twice until the final pass replaces it.
fn until_repeat(text: &str) -> &str {
    const RUN: usize = 4;
    let norm = |w: &str| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let words: Vec<(usize, String)> = text.split_whitespace().map(|w| (w.as_ptr() as usize - text.as_ptr() as usize, norm(w))).collect();
    let mut seen = std::collections::HashSet::new();
    for i in 0..words.len().saturating_sub(RUN - 1) {
        let run: Vec<&str> = words[i..i + RUN].iter().map(|(_, w)| w.as_str()).collect();
        if !seen.insert(run) {
            return text[..words[i].0].trim_end().trim_end_matches([',', ';', ':', '-']).trim_end();
        }
    }
    text
}

/// Audio up to this long is handed to the engine in one call (one Whisper
/// window); longer audio is cut at pauses first, see [`transcribe_all`].
pub const ONE_PASS_SECS: f32 = 30.0;

/// Transcribes a whole recording at once (no streaming) and returns raw text.
///
/// Whisper decodes audio over 30 s window by window, and whisper.cpp hands
/// each window the text of the window before as its prompt, even with
/// `no_context` set (it only clears the history once, before the first
/// window). With a dictionary or context prompt that made the second window
/// repeat the first one's last sentence. Longer audio is therefore cut at
/// pauses exactly like a streaming [`Session`], so every engine call is a
/// single window that sees only the user's prompt.
pub fn transcribe_all(engine: &mut dyn Engine, samples: &[f32], lang: Language, prompt: &str) -> Result<String, EngineError> {
    if samples.len() as f32 <= ONE_PASS_SECS * RATE as f32 {
        return engine.transcribe(samples, lang, prompt);
    }
    let mut session = Session::new(lang);
    session.push(samples);
    // `finish` cuts chunks until the rest fits one final pass, so no chunk is decoded twice.
    session.finish(engine, prompt)
}

/// Chooses where to end a chunk of `pending` audio: the middle of the latest
/// pause that leaves at least [`MIN_CHUNK_SECS`] before it, within
/// [`MAX_CHUNK_SECS`]; with no such pause and a full [`MAX_CHUNK_SECS`] waiting,
/// the quietest frame in that range. `None` means wait for more audio.
pub fn split_point(pending: &[f32]) -> Option<usize> {
    let min = (MIN_CHUNK_SECS * RATE as f32) as usize;
    let max = (MAX_CHUNK_SECS * RATE as f32) as usize;
    if pending.len() < min {
        return None;
    }
    let window = &pending[..pending.len().min(max)];
    let activity = VoiceActivity::analyze(window);
    let voiced = activity.frames();
    let min_pause = (MIN_PAUSE_SECS * RATE as f32 / FRAME as f32).ceil() as usize;

    let mut best = None;
    let mut run_start = None;
    for i in 0..=voiced.len() {
        let silent = i < voiced.len() && !voiced[i];
        match (silent, run_start) {
            (true, None) => run_start = Some(i),
            (false, Some(s)) => {
                let mid = (s + i) / 2 * FRAME;
                if i - s >= min_pause && mid >= min {
                    best = Some(mid);
                }
                run_start = None;
            }
            _ => {}
        }
    }
    if best.is_some() || pending.len() < max {
        return best;
    }
    // No pause in a full window: cut at the quietest frame after the minimum.
    window[min..]
        .chunks(FRAME)
        .enumerate()
        .filter(|(_, f)| f.len() == FRAME)
        .min_by(|a, b| energy(a.1).total_cmp(&energy(b.1)))
        .map(|(i, _)| min + i * FRAME + FRAME / 2)
}

fn energy(frame: &[f32]) -> f32 {
    frame.iter().map(|s| s * s).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records what it was asked to transcribe; answers "c<n>".
    struct Recorder {
        calls: Vec<usize>,
        prompts: Vec<String>,
    }

    impl Engine for Recorder {
        fn name(&self) -> &str {
            "recorder"
        }
        fn transcribe(&mut self, samples: &[f32], _: Language, prompt: &str) -> Result<String, EngineError> {
            self.calls.push(samples.len());
            self.prompts.push(prompt.to_string());
            Ok(format!("c{}", self.calls.len()))
        }
    }

    /// Syllable-like speech: 150 ms bursts every 250 ms over a quiet floor.
    fn speech(secs: f32) -> Vec<f32> {
        (0..(secs * RATE as f32) as usize)
            .map(|i| {
                let noise = ((i * 7919) % 101) as f32 / 101.0 * 0.001;
                if i % (RATE / 4) < RATE * 3 / 20 {
                    noise + 0.2 * (i as f32 * 0.09).sin()
                } else {
                    noise
                }
            })
            .collect()
    }

    fn pause(secs: f32) -> Vec<f32> {
        (0..(secs * RATE as f32) as usize).map(|i| ((i * 7919) % 101) as f32 / 101.0 * 0.001).collect()
    }

    fn secs(n: usize) -> f32 {
        n as f32 / RATE as f32
    }

    #[test]
    fn repeated_preview_text_is_cut() {
        assert_eq!(
            until_repeat("Ez az új környezet különböző forrásokkal és különböző forrásokkal és különböző forrásokkal."),
            "Ez az új környezet különböző forrásokkal és"
        );
        assert_eq!(until_repeat("A férfiak is sürgették. A férfiak is sürgették."), "A férfiak is sürgették.");
        assert_eq!(until_repeat("egy kettő három, egy kettő négy"), "egy kettő három, egy kettő négy");
        assert_eq!(until_repeat(""), "");
    }

    #[test]
    fn previews_feed_the_caption_but_never_the_result() {
        let mut e = Recorder { calls: vec![], prompts: vec![] };
        let stop = AtomicBool::new(false);
        let mut s = Session::new(Language::Hungarian);
        s.push(&speech(3.5));
        assert!(!s.preview(&mut e, "", &stop).unwrap(), "under the preview minimum");
        s.push(&speech(1.0));
        assert!(s.preview(&mut e, "", &stop).unwrap());
        assert_eq!(s.caption(), "c1");
        assert_eq!(s.text(), "", "a preview is not a chunk");
        s.push(&speech(2.0));
        assert!(!s.preview(&mut e, "", &stop).unwrap(), "next preview after 3 s more");
        s.push(&speech(1.0));
        assert!(s.preview(&mut e, "", &stop).unwrap());
        assert_eq!(s.caption(), "c2");
        assert_eq!(e.calls, [secs_to(4.5), secs_to(7.5)], "each preview covers all waiting audio");
        stop.store(true, Ordering::Relaxed);
        s.push(&speech(3.0));
        assert!(!s.preview(&mut e, "", &stop).unwrap(), "stopped");
        assert_eq!(s.finish(&mut e, "").unwrap(), "c3", "the final pass ignores previews");
        assert_eq!(e.calls[2], secs_to(10.5));
    }

    #[test]
    fn no_preview_while_the_speaker_is_silent() {
        let mut e = Recorder { calls: vec![], prompts: vec![] };
        let stop = AtomicBool::new(false);
        let mut s = Session::new(Language::Hungarian);
        s.push(&speech(4.5));
        s.push(&pause(0.6));
        assert!(!s.preview(&mut e, "", &stop).unwrap(), "likely about to release");
        s.push(&speech(0.5));
        assert!(s.preview(&mut e, "", &stop).unwrap());
    }

    #[test]
    fn a_chunk_replaces_the_preview_it_covers() {
        let mut e = Recorder { calls: vec![], prompts: vec![] };
        let stop = AtomicBool::new(false);
        let mut s = Session::new(Language::Hungarian);
        s.push(&speech(8.5));
        assert!(s.preview(&mut e, "", &stop).unwrap());
        s.push(&pause(0.5));
        s.push(&speech(1.0));
        assert!(s.step(&mut e, "").unwrap());
        assert_eq!(s.caption(), "c2", "the old preview no longer shows");
        s.push(&speech(3.0));
        assert!(s.preview(&mut e, "", &stop).unwrap());
        assert_eq!(s.caption(), "c2 c3");
    }

    fn secs_to(s: f32) -> usize {
        (s * RATE as f32) as usize
    }

    #[test]
    fn short_audio_waits() {
        assert_eq!(split_point(&speech(5.0)), None);
        let mut a = speech(4.0);
        a.extend(pause(1.0));
        a.extend(speech(2.0));
        assert_eq!(split_point(&a), None, "pause before the minimum chunk");
    }

    #[test]
    fn splits_in_the_latest_pause() {
        let mut a = speech(9.0);
        a.extend(pause(0.6));
        a.extend(speech(3.0));
        a.extend(pause(0.6));
        a.extend(speech(1.0));
        let cut = secs(split_point(&a).unwrap());
        assert!((12.6..13.2).contains(&cut), "cut at {cut}");
    }

    #[test]
    fn short_gaps_between_syllables_are_not_pauses() {
        assert_eq!(split_point(&speech(20.0)), None);
    }

    #[test]
    fn forces_a_cut_without_pauses() {
        let a = speech(MAX_CHUNK_SECS + 1.0);
        let cut = secs(split_point(&a).unwrap());
        assert!((MIN_CHUNK_SECS..=MAX_CHUNK_SECS).contains(&cut), "cut at {cut}");
    }

    #[test]
    fn session_streams_chunks_and_finishes_with_the_tail() {
        let mut eng = Recorder { calls: Vec::new(), prompts: Vec::new() };
        let mut s = Session::new(Language::Hungarian);
        let mut audio = Vec::new();
        for _ in 0..4 {
            audio.extend(speech(9.0));
            audio.extend(pause(0.6));
        }
        audio.extend(speech(9.0));
        // Feed in 100 ms pieces and step once a second, as the app does.
        for (i, piece) in audio.chunks(RATE / 10).enumerate() {
            s.push(piece);
            if i % 10 == 9 {
                s.step(&mut eng, "Pusmogo").unwrap();
            }
        }
        assert_eq!(s.chunks_done(), 4, "one chunk per finished sentence");
        assert!(s.pending_secs() < 10.0, "tail {}", s.pending_secs());
        // 48 s in all: the final pass redoes the last two chunks with the tail
        // (under 29 s together) and keeps the first two.
        assert_eq!(s.finish(&mut eng, "Pusmogo").unwrap(), "c1 c2 c5");
        let last = *eng.calls.last().unwrap() as f32 / RATE as f32;
        assert!(last > 20.0 && last <= FINAL_PASS_SECS, "final pass {last} s");
        assert_eq!(s.finish(&mut eng, "").unwrap(), "c1 c2 c5", "finish is idempotent");
        assert_eq!(eng.calls.len(), 5);
        assert!(eng.prompts.iter().all(|p| p == "Pusmogo"));
    }

    #[test]
    fn a_dictation_under_one_window_is_one_pass() {
        let mut eng = Recorder { calls: Vec::new(), prompts: Vec::new() };
        let mut s = Session::new(Language::Hungarian);
        let mut audio = speech(9.0);
        audio.extend(pause(0.6));
        audio.extend(speech(9.0));
        audio.extend(pause(0.6));
        audio.extend(speech(5.0));
        s.push(&audio);
        while s.step(&mut eng, "").unwrap() {}
        assert!(s.chunks_done() >= 1);
        let n = eng.calls.len() + 1;
        assert_eq!(s.finish(&mut eng, "").unwrap(), format!("c{n}"));
        assert_eq!(*eng.calls.last().unwrap(), audio.len(), "same as transcribing the whole clip");
    }

    #[test]
    fn a_short_tail_is_joined_to_the_last_chunk() {
        let mut eng = Recorder { calls: Vec::new(), prompts: Vec::new() };
        let mut s = Session::new(Language::Hungarian);
        let mut audio = speech(9.0);
        audio.extend(pause(0.6));
        s.push(&audio);
        assert!(s.step(&mut eng, "").unwrap());
        let first = eng.calls[0];
        s.push(&speech(2.0));
        assert_eq!(s.finish(&mut eng, "").unwrap(), "c2", "the first chunk was redone with the tail");
        assert_eq!(eng.calls[1], audio.len() + 2 * RATE);
        assert!(first < eng.calls[1]);
    }

    #[test]
    fn empty_chunks_leave_no_gaps() {
        struct Quiet;
        impl Engine for Quiet {
            fn name(&self) -> &str {
                "quiet"
            }
            fn transcribe(&mut self, _: &[f32], _: Language, _: &str) -> Result<String, EngineError> {
                Ok(String::new())
            }
        }
        let mut s = Session::new(Language::English);
        s.push(&speech(3.0));
        assert_eq!(s.finish(&mut Quiet, "").unwrap(), "");
    }

    #[test]
    fn long_audio_is_cut_at_pauses_and_each_part_gets_the_prompt() {
        let mut audio = Vec::new();
        for _ in 0..4 {
            audio.extend(speech(9.0));
            audio.extend(pause(0.6));
        }
        audio.extend(speech(9.0));
        let mut eng = Recorder { calls: Vec::new(), prompts: Vec::new() };
        assert_eq!(transcribe_all(&mut eng, &audio, Language::Hungarian, "Pusmogo").unwrap(), "c1 c2");
        // 48 s: one chunk at a pause, then the rest (under 29 s) in one pass.
        assert_eq!(eng.calls.len(), 2);
        assert!(eng.calls.iter().all(|&n| n as f32 <= FINAL_PASS_SECS * RATE as f32), "{:?}", eng.calls);
        assert_eq!(eng.calls.iter().sum::<usize>(), audio.len(), "every sample once");
        assert!(eng.prompts.iter().all(|p| p == "Pusmogo"));
    }

    #[test]
    fn a_release_abandons_a_chunk_the_final_pass_redoes() {
        let mut eng = Recorder { calls: Vec::new(), prompts: Vec::new() };
        let mut s = Session::new(Language::Hungarian);
        let mut audio = speech(9.0);
        audio.extend(pause(0.6));
        audio.extend(speech(1.0));
        s.push(&audio);
        let released = AtomicBool::new(true);
        assert!(!s.step_until(&mut eng, "", &released).unwrap(), "the chunk pass gave up");
        assert_eq!(s.chunks_done(), 0);
        assert_eq!(s.finish(&mut eng, "").unwrap(), "c1");
        assert_eq!(eng.calls, vec![audio.len()], "one final pass, as if the step never ran");
    }

    #[test]
    fn a_release_keeps_a_chunk_further_back() {
        let mut eng = Recorder { calls: Vec::new(), prompts: Vec::new() };
        let mut s = Session::new(Language::Hungarian);
        let mut audio = speech(9.0);
        audio.extend(pause(0.6));
        audio.extend(speech(FINAL_PASS_SECS));
        s.push(&audio);
        let released = AtomicBool::new(true);
        assert!(s.step_until(&mut eng, "", &released).unwrap(), "finish would not redo it, so it runs");
        assert_eq!(s.chunks_done(), 1);
    }

    #[test]
    fn finish_cuts_more_than_one_pass_of_waiting_audio() {
        let mut eng = Recorder { calls: Vec::new(), prompts: Vec::new() };
        let mut s = Session::new(Language::Hungarian);
        let mut audio = Vec::new();
        for _ in 0..4 {
            audio.extend(speech(9.0));
            audio.extend(pause(0.6));
        }
        s.push(&audio);
        // No steps ran while recording (as after an abandoned chunk pass).
        assert_eq!(s.finish(&mut eng, "").unwrap(), "c1 c2");
        assert!(eng.calls.iter().all(|&n| n as f32 <= FINAL_PASS_SECS * RATE as f32), "{:?}", eng.calls);
        assert_eq!(eng.calls.iter().sum::<usize>(), audio.len(), "every sample once");
    }

    #[test]
    fn audio_within_one_window_is_one_call() {
        let audio = speech(ONE_PASS_SECS);
        let mut eng = Recorder { calls: Vec::new(), prompts: Vec::new() };
        assert_eq!(transcribe_all(&mut eng, &audio, Language::Hungarian, "").unwrap(), "c1");
        assert_eq!(eng.calls, vec![audio.len()]);
    }
    /// Answers with a fingerprint of the audio it was given, so two runs
    /// match only if every pass read exactly the same samples.
    struct Fingerprint(Vec<String>);

    impl Engine for Fingerprint {
        fn name(&self) -> &str {
            "fingerprint"
        }
        fn transcribe(&mut self, samples: &[f32], _: Language, _: &str) -> Result<String, EngineError> {
            let sum: f64 = samples.iter().enumerate().map(|(i, &v)| v as f64 * (1 + i % 97) as f64).sum();
            let text = format!("{}:{sum:.6}", samples.len());
            self.0.push(text.clone());
            Ok(text)
        }
    }

    /// A ten-minute hands-free dictation: sentences of varying length and
    /// loudness, stepped once a second with previews, as the app does.
    fn long_session(keep_all: bool) -> (String, Vec<String>, f32, f32) {
        let mut audio = Vec::new();
        let mut n = 0;
        while secs(audio.len()) < 600.0 {
            let gain = 0.5 + (n % 5) as f32 * 0.25;
            audio.extend(speech(6.0 + (n % 7) as f32 * 3.0).into_iter().map(|v| v * gain));
            audio.extend(pause(0.4 + (n % 3) as f32 * 0.3));
            n += 1;
        }
        let mut eng = Fingerprint(Vec::new());
        let mut s = Session::new(Language::Hungarian);
        s.keep_all = keep_all;
        let stop = AtomicBool::new(false);
        let mut most = 0f32;
        for (i, piece) in audio.chunks(RATE / 10).enumerate() {
            s.push(piece);
            if i % 10 == 9 && !s.step_until(&mut eng, "", &stop).unwrap() {
                s.preview(&mut eng, "", &stop).unwrap();
            }
            most = most.max(s.kept_secs());
        }
        let total = s.total_secs();
        (s.finish(&mut eng, "").unwrap(), eng.0, most, total)
    }

    #[test]
    fn a_long_dictation_keeps_only_the_audio_a_pass_can_still_read() {
        let (kept_text, kept_calls, kept_most, total) = long_session(true);
        let (text, calls, most, _) = long_session(false);
        assert_eq!(text, kept_text, "same result as with every sample kept");
        assert_eq!(calls, kept_calls, "every pass read the same audio");
        assert!(kept_most >= total - 1.0, "keep_all keeps everything ({kept_most} of {total} s)");
        let bound = FINAL_PASS_SECS + MAX_CHUNK_SECS + DROP_SECS + 1.0;
        assert!(most <= bound, "at most {most} s kept of {total} s (bound {bound} s)");
    }
}
