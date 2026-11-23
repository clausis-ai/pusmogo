//! Speech engines. Every engine takes 16 kHz mono f32 samples and returns raw text.
//! Cleanup happens afterwards in `cleanup`, the same for every engine.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Auto,
    Hungarian,
    English,
}

impl Language {
    pub fn from_code(code: &str) -> Self {
        match code.trim().to_ascii_lowercase().as_str() {
            "hu" | "hun" | "hungarian" | "magyar" => Language::Hungarian,
            "en" | "eng" | "english" | "angol" => Language::English,
            _ => Language::Auto,
        }
    }

    pub fn whisper_code(self) -> Option<&'static str> {
        match self {
            Language::Auto => None,
            Language::Hungarian => Some("hu"),
            Language::English => Some("en"),
        }
    }
}

#[derive(Debug)]
pub enum EngineError {
    ModelLoad(String),
    Inference(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::ModelLoad(m) => write!(f, "model load failed: {m}"),
            EngineError::Inference(m) => write!(f, "inference failed: {m}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// Whisper keeps at most this many prompt tokens (half its 448-token text
/// context) and drops the rest from the front.
pub const MAX_PROMPT_TOKENS: usize = 224;

/// A speech-to-text engine. `prompt` carries the personal dictionary / vocabulary
/// so English terms inside Hungarian speech keep their spelling.
pub trait Engine: Send {
    fn name(&self) -> &str;
    fn transcribe(&mut self, samples: &[f32], lang: Language, prompt: &str) -> Result<String, EngineError>;

    /// Like [`Engine::transcribe`], but gives up as soon as `stop` is set and
    /// returns `None` then. Used for caption previews, which must never keep a
    /// final pass waiting. Engines that cannot stop midway check `stop` once.
    fn transcribe_until(
        &mut self,
        samples: &[f32],
        lang: Language,
        prompt: &str,
        stop: &AtomicBool,
    ) -> Result<Option<String>, EngineError> {
        if stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        self.transcribe(samples, lang, prompt).map(Some)
    }

    /// Like [`Engine::transcribe`] (same decoding, same state, same text),
    /// but gives up when `stop` is set and returns `None` then. Used for chunk
    /// passes that a key release may abandon because the final pass redoes
    /// their audio anyway. Engines that cannot stop midway check `stop` once.
    fn transcribe_abortable(
        &mut self,
        samples: &[f32],
        lang: Language,
        prompt: &str,
        stop: &AtomicBool,
    ) -> Result<Option<String>, EngineError> {
        if stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        self.transcribe(samples, lang, prompt).map(Some)
    }

    /// Runs the model once on synthetic input so one-time setup (GPU pipelines,
    /// buffers) happens before the first real dictation. The result is discarded.
    fn warm_up(&mut self) -> Result<(), EngineError> {
        Ok(())
    }

    /// How many engine tokens `text` takes, if the engine has a tokenizer.
    /// Whisper keeps only the last 224 prompt tokens, so callers use this to
    /// see whether a dictionary prompt fits.
    fn count_tokens(&self, _text: &str) -> Option<usize> {
        None
    }
}

/// Used when no model is installed yet, in tests and in CI. Reports what it heard
/// so the whole hotkey → record → paste loop can be tested without a 500 MB model.
pub struct StubEngine;

impl Engine for StubEngine {
    fn name(&self) -> &str {
        "stub"
    }

    fn transcribe(&mut self, samples: &[f32], lang: Language, _prompt: &str) -> Result<String, EngineError> {
        let secs = samples.len() as f32 / crate::SAMPLE_RATE as f32;
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        Ok(format!(
            "[pusmogo stub: {secs:.1} s audio, peak {peak:.2}, lang {lang:?}, no model loaded]"
        ))
    }
}

/// Loads the best available engine: Whisper when compiled in and the model file
/// exists, otherwise the stub.
pub fn load(model_path: Option<&str>) -> Result<Box<dyn Engine>, EngineError> {
    #[cfg(feature = "whisper")]
    if let Some(path) = model_path.filter(|p| std::path::Path::new(p).exists()) {
        return Ok(Box::new(whisper::WhisperEngine::new(path)?));
    }
    let _ = model_path;
    Ok(Box::new(StubEngine))
}

/// Encoder frames for a caption preview of `samples` (16 kHz): the audio's
/// frames (50 per second) plus a 64-frame margin, rounded up to 64, at least
/// 768; 0 (Whisper's full 30 s window) when that would be most of it anyway.
/// Below the full window the decoder sometimes repeats a sentence
/// (`docs/bench/audioctx-01.md`), so final passes never use this.
/// `PUSMOGO_PREVIEW_FULL_WINDOW=1` turns it off for comparisons.
pub fn preview_audio_ctx(samples: usize) -> i32 {
    const FULL: usize = 1500;
    if std::env::var_os("PUSMOGO_PREVIEW_FULL_WINDOW").is_some_and(|v| v == "1") {
        return 0;
    }
    let frames = samples.div_ceil(crate::SAMPLE_RATE as usize / 50) + 64;
    let ctx = frames.div_ceil(64).max(12) * 64;
    if ctx >= FULL - 64 { 0 } else { ctx as i32 }
}

#[cfg(feature = "whisper")]
pub mod whisper {
    use super::{preview_audio_ctx, Engine, EngineError, Language};
    use std::sync::atomic::{AtomicBool, Ordering};
    use whisper_rs::{
        FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState, WhisperVadContext,
        WhisperVadContextParams, WhisperVadParams,
    };

    pub struct WhisperEngine {
        // Kept alive for the state's lifetime.
        ctx: WhisperContext,
        state: WhisperState,
        /// A second state for caption previews, made at the first preview. A
        /// pass with a short encoder window changes what later full-window
        /// passes on the same state decode (30 of 100 FLEURS results), so
        /// previews never share the state final passes use.
        preview_state: Option<WhisperState>,
        detector: Option<SpeechDetector>,
        /// Tokens Hungarian dictation never writes ([`crate::alphabet`]); the
        /// logits filter reads it through a pointer during `full()`.
        hu_blocked: Vec<i32>,
    }

    /// Silero speech detector run by whisper.cpp on the CPU, about 2 ms per
    /// second of audio. Room noise, typing and chatter that pass the energy
    /// check cost a full Whisper pass (0.65 s for 4 s); this skips it.
    struct SpeechDetector(WhisperVadContext);

    // SAFETY: the context is owned by one engine and only used through
    // `&mut self`, so it is never touched from two threads at once.
    unsafe impl Send for SpeechDetector {}

    impl SpeechDetector {
        /// Loads the detector next to the Whisper model, if it is there.
        /// `PUSMOGO_SPEECH_DETECTOR=0` turns it off (for A/B runs).
        fn beside(model_path: &str) -> Option<Self> {
            if std::env::var("PUSMOGO_SPEECH_DETECTOR").is_ok_and(|v| v == "0") {
                return None;
            }
            let path = std::path::Path::new(model_path).with_file_name(crate::vad::DETECTOR_MODEL_FILE);
            if !path.exists() {
                return None;
            }
            let mut params = WhisperVadContextParams::new();
            params.set_use_gpu(false);
            params.set_n_threads(2);
            let detector = WhisperVadContext::new(path.to_str()?, params).ok().map(Self);
            eprintln!("pusmogo: speech detector {}", if detector.is_some() { "loaded" } else { "failed to load" });
            detector
        }

        /// False only when the detector ran and found no speech at all.
        fn hears_speech(&mut self, samples: &[f32]) -> bool {
            let input = crate::vad::detector_input(samples);
            match self.0.segments_from_samples(WhisperVadParams::new(), &input) {
                Ok(segments) => segments.num_segments() > 0,
                Err(_) => true,
            }
        }
    }

    impl WhisperEngine {
        pub fn new(model_path: &str) -> Result<Self, EngineError> {
            // Flash attention: ~5 % faster on Metal, accuracy unchanged on
            // synthetic and FLEURS Hungarian (docs/bench/flashattn-01.md).
            let mut ctx_params = WhisperContextParameters::default();
            ctx_params.flash_attn(true);
            let ctx = WhisperContext::new_with_params(model_path, ctx_params)
                .map_err(|e| EngineError::ModelLoad(e.to_string()))?;
            let state = ctx.create_state().map_err(|e| EngineError::ModelLoad(e.to_string()))?;
            let hu_blocked = hungarian_blocked_tokens(&ctx);
            Ok(Self { ctx, state, preview_state: None, detector: SpeechDetector::beside(model_path), hu_blocked })
        }
    }

    /// Token ids whose bytes start a non-Latin character, only with
    /// `PUSMOGO_HU_ALPHABET=1`. Off by default: base turbo wrote no non-Latin
    /// character on 288 Hungarian clips, normal and whispered, and blocking the
    /// tokens still shifted beam scores (WER +0.3 to +0.6 points, within noise).
    fn hungarian_blocked_tokens(ctx: &WhisperContext) -> Vec<i32> {
        if !std::env::var("PUSMOGO_HU_ALPHABET").is_ok_and(|v| v == "1") {
            return Vec::new();
        }
        (0..ctx.n_vocab())
            .filter(|&id| ctx.token_to_bytes(id).is_ok_and(crate::alphabet::foreign_script))
            .collect()
    }

    unsafe extern "C" fn block_tokens(
        _ctx: *mut whisper_rs::WhisperSysContext,
        _state: *mut whisper_rs::WhisperSysState,
        _tokens: *const whisper_rs::WhisperTokenData,
        _n_tokens: std::os::raw::c_int,
        logits: *mut f32,
        user_data: *mut std::ffi::c_void,
    ) {
        // SAFETY: `user_data` is the engine's token list, alive and unchanged for the whole `full()` call,
        // and every id in it is below n_vocab, the length of `logits`.
        unsafe {
            let blocked = &*(user_data as *const Vec<i32>);
            for &id in blocked {
                *logits.add(id as usize) = f32::NEG_INFINITY;
            }
        }
    }

    impl Engine for WhisperEngine {
        fn name(&self) -> &str {
            "whisper"
        }

        fn transcribe(&mut self, samples: &[f32], lang: Language, prompt: &str) -> Result<String, EngineError> {
            Ok(self.transcribe_inner(samples, lang, prompt, None, false)?.unwrap_or_default())
        }

        fn transcribe_abortable(
            &mut self,
            samples: &[f32],
            lang: Language,
            prompt: &str,
            stop: &AtomicBool,
        ) -> Result<Option<String>, EngineError> {
            if stop.load(Ordering::Relaxed) {
                return Ok(None);
            }
            self.transcribe_inner(samples, lang, prompt, Some(stop), false)
        }

        fn transcribe_until(
            &mut self,
            samples: &[f32],
            lang: Language,
            prompt: &str,
            stop: &AtomicBool,
        ) -> Result<Option<String>, EngineError> {
            let mut preview = match self.preview_state.take() {
                Some(state) => state,
                None => self.ctx.create_state().map_err(|e| EngineError::Inference(e.to_string()))?,
            };
            std::mem::swap(&mut self.state, &mut preview);
            let result = self.transcribe_inner(samples, lang, prompt, Some(stop), true);
            std::mem::swap(&mut self.state, &mut preview);
            self.preview_state = Some(preview);
            result
        }

        fn warm_up(&mut self) -> Result<(), EngineError> {
            // Silence would be skipped by the speech check in `transcribe`, so the
            // model would never run: feed one second of a quiet tone directly.
            let tone: Vec<f32> = (0..crate::SAMPLE_RATE)
                .map(|i| 0.1 * (i as f32 * 2.0 * std::f32::consts::PI * 220.0 / crate::SAMPLE_RATE as f32).sin())
                .collect();
            self.run(&tone, Language::Hungarian, "", None, false).map(|_| ())
        }

        fn count_tokens(&self, text: &str) -> Option<usize> {
            self.ctx.tokenize(text, text.len() + 16).ok().map(|t| t.len())
        }
    }

    impl WhisperEngine {
        /// `None` only when `stop` was set during the pass. `preview` passes
        /// decode greedily with a short encoder window (caption only).
        fn transcribe_inner(
            &mut self,
            samples: &[f32],
            lang: Language,
            prompt: &str,
            stop: Option<&AtomicBool>,
            preview: bool,
        ) -> Result<Option<String>, EngineError> {
            // No speech in the clip: skip inference, which would only invent text.
            let activity = crate::vad::VoiceActivity::analyze(samples);
            if !activity.has_speech() {
                return Ok(Some(String::new()));
            }
            // Loud sounds without a voice (fan, typing, chatter): the speech
            // detector is far cheaper than a Whisper pass that drops its text.
            if self.detector.as_mut().is_some_and(|d| !d.hears_speech(samples)) {
                return Ok(Some(String::new()));
            }
            let Some(out) = self.decode(samples, lang, prompt, &activity, stop, preview)? else { return Ok(None) };
            // With a prompt Whisper sometimes decodes a sentence said twice
            // only once. When the text is far too short for the voiced audio,
            // decode once more without the prompt and keep that if it recovered
            // the missing speech.
            if !prompt.is_empty() && crate::vad::prompt_skipped_speech(&out, activity.voiced_secs()) {
                let Some(plain) = self.decode(samples, lang, "", &activity, stop, preview)? else { return Ok(None) };
                if crate::vad::prefer_plain(&out, &plain) {
                    return Ok(Some(plain));
                }
            }
            Ok(Some(out))
        }

        /// One engine pass, with the hallucination guard applied per segment.
        fn decode(
            &mut self,
            samples: &[f32],
            lang: Language,
            prompt: &str,
            activity: &crate::vad::VoiceActivity,
            stop: Option<&AtomicBool>,
            preview: bool,
        ) -> Result<Option<String>, EngineError> {
            if !self.run(samples, lang, prompt, stop, preview)? {
                return Ok(None);
            }
            let mut out = String::new();
            for segment in self.state.as_iter() {
                let text = segment.to_str_lossy().map_err(|e| EngineError::Inference(e.to_string()))?;
                // Timestamps are in centiseconds. A known silence phrase over a
                // silent span (e.g. the short tail window after 30 s), or one
                // decoded with low confidence over room noise, is dropped.
                let (t0, t1) = (segment.start_timestamp() as f32 / 100.0, segment.end_timestamp() as f32 / 100.0);
                let eot = self.ctx.token_eot();
                let probs: Vec<f32> = (0..segment.n_tokens())
                    .filter_map(|i| segment.get_token(i))
                    .filter(|t| t.token_id() < eot)
                    .map(|t| t.token_probability())
                    .collect();
                let confidence = if probs.is_empty() { 1.0 } else { probs.iter().sum::<f32>() / probs.len() as f32 };
                if crate::vad::drop_segment(&text, activity.voiced_secs_between(t0, t1), confidence)
                    || (!prompt.is_empty() && crate::vad::echoes_prompt_label(&text))
                {
                    continue;
                }
                out.push_str(&text);
            }
            Ok(Some(out.trim().to_string()))
        }

        /// Returns false when `stop` was set and the pass gave up.
        fn run(
            &mut self,
            samples: &[f32],
            lang: Language,
            prompt: &str,
            stop: Option<&AtomicBool>,
            preview: bool,
        ) -> Result<bool, EngineError> {
            // Final passes under BEAM_MAX_SECS use beam search: on FLEURS, short
            // phrases and the synthetic sets it lowers WER for ~70 ms per pass
            // (docs/bench/beam-01.md). Near-30 s windows stay greedy: there beam
            // search dropped a whole sentence. Previews stay greedy, they are
            // shown for a moment and must be cheap.
            let beam = super::beam_size(std::env::var("PUSMOGO_BEAM").ok().as_deref());
            let short = samples.len() < super::BEAM_MAX_SECS * crate::SAMPLE_RATE as usize;
            let strategy = if beam > 1 && short && !preview {
                SamplingStrategy::BeamSearch { beam_size: beam, patience: -1.0 }
            } else {
                SamplingStrategy::Greedy { best_of: 1 }
            };
            let mut params = FullParams::new(strategy);
            params.set_language(lang.whisper_code());
            params.set_translate(false);
            params.set_no_context(true);
            params.set_single_segment(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            params.set_print_special(false);
            params.set_suppress_blank(true);
            if !prompt.is_empty() {
                params.set_initial_prompt(prompt);
            }
            params.set_n_threads(std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4).min(8));

            if preview {
                // Caption previews are shown for a moment and never typed, so
                // they use a shorter encoder window (`preview_audio_ctx`): about
                // twice as fast, which is what a key release waits for when it
                // lands during a preview.
                params.set_audio_ctx(preview_audio_ctx(samples.len()));
            }
            if lang == Language::Hungarian && !self.hu_blocked.is_empty() {
                // SAFETY: the list lives in `self`, which neither moves nor changes while `full()` runs; the filter only reads it.
                unsafe {
                    params.set_filter_logits_callback(Some(block_tokens));
                    params.set_filter_logits_callback_user_data(&self.hu_blocked as *const Vec<i32> as *mut std::ffi::c_void);
                }
            }
            if let Some(stop) = stop {
                unsafe extern "C" fn should_abort(flag: *mut std::ffi::c_void) -> bool {
                    // SAFETY: `flag` is the `&AtomicBool` set below, alive for the whole `full()` call.
                    unsafe { (*(flag as *const AtomicBool)).load(Ordering::Relaxed) }
                }
                // SAFETY: the callback only reads the flag, which outlives `full()`.
                unsafe {
                    params.set_abort_callback(Some(should_abort));
                    params.set_abort_callback_user_data(stop as *const AtomicBool as *mut std::ffi::c_void);
                }
            }

            let result = self.state.full(params, samples);
            if stop.is_some_and(|s| s.load(Ordering::Relaxed)) {
                return Ok(false);
            }
            result.map_err(|e| EngineError::Inference(e.to_string()))?;
            Ok(true)
        }
    }
}

/// Beam width for final passes: [`DEFAULT_BEAM`] unless `PUSMOGO_BEAM` sets
/// another (1 = greedy, for A/B runs). Values outside 1..=8 are ignored.
pub fn beam_size(env: Option<&str>) -> i32 {
    env.and_then(|v| v.trim().parse::<i32>().ok()).filter(|n| (1..=8).contains(n)).unwrap_or(DEFAULT_BEAM)
}

/// See `docs/bench/beam-01.md`.
pub const DEFAULT_BEAM: i32 = 5;

/// Longer passes decode greedily (see `docs/bench/beam-01.md`).
pub const BEAM_MAX_SECS: usize = 20;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes() {
        assert_eq!(Language::from_code("HU"), Language::Hungarian);
        assert_eq!(Language::from_code("en"), Language::English);
        assert_eq!(Language::from_code(""), Language::Auto);
        assert_eq!(Language::Hungarian.whisper_code(), Some("hu"));
    }

    #[test]
    fn preview_window_fits_the_audio() {
        let secs = |s: f32| (s * crate::SAMPLE_RATE as f32) as usize;
        assert_eq!(preview_audio_ctx(secs(4.0)), 768, "never below the floor");
        assert_eq!(preview_audio_ctx(secs(15.0)), 832, "750 frames + margin, rounded up to 64");
        assert_eq!(preview_audio_ctx(secs(28.0)), 0, "near 30 s: the full window");
    }

    #[test]
    fn beam_size_defaults_and_overrides() {
        assert_eq!(beam_size(None), DEFAULT_BEAM);
        assert_eq!(beam_size(Some("1")), 1, "greedy for A/B");
        assert_eq!(beam_size(Some(" 3 ")), 3);
        assert_eq!(beam_size(Some("0")), DEFAULT_BEAM);
        assert_eq!(beam_size(Some("99")), DEFAULT_BEAM);
        assert_eq!(beam_size(Some("x")), DEFAULT_BEAM);
    }

    #[test]
    fn stub_reports_duration() {
        let mut e = load(None).unwrap();
        let text = e.transcribe(&vec![0.5; 16_000], Language::Hungarian, "").unwrap();
        assert!(text.contains("1.0 s"), "{text}");
    }
}
