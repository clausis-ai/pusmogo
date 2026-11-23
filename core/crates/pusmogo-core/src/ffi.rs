//! C ABI used by the Swift app. Header: `core/include/pusmogo.h` (keep in sync).
//!
//! Ownership: every `*mut c_char` returned here must be freed with
//! `pusmogo_string_free`. The engine handle is freed with `pusmogo_engine_free`.
//! A handle must not be used from two threads at once (the app uses one serial queue).
//!
//! # Safety (all functions)
//! Pointers must be NULL or valid for the stated length; strings must be
//! NUL-terminated UTF-8; handles must come from `pusmogo_engine_new` and not be freed twice.
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cleanup::{clean, Dictionary};
use crate::engine::{self, Engine, Language};
use crate::models;
use crate::profile::{self, Profile};
use crate::session::Session;

pub struct PusmogoEngine {
    engine: Box<dyn Engine>,
    dict: Dictionary,
    /// Hint words from the text around the cursor, for the current dictation
    /// only (`pusmogo_set_context`). Held in memory, never stored.
    context: Vec<String>,
    /// Folder for the opt-in record of recent dictations (`pusmogo_set_debug_dir`).
    debug_dir: Option<std::path::PathBuf>,
    /// Drop spoken self-corrections after cleanup (`backtrack`), on unless
    /// `pusmogo_set_self_corrections` turns it off.
    self_corrections: bool,
    /// The last finished dictation's text before the self-correction pass,
    /// when that pass dropped something (`pusmogo_last_spoken`); else None.
    last_spoken: Option<String>,
}

/// The text the app types: cleanup with the dictionary and context hints, for
/// Hungarian the spelling repair, then (when on) spoken self-corrections dropped.
fn finish_text(h: &PusmogoEngine, raw: &str, lang: Language) -> String {
    finish_parts(h, raw, lang).0
}

/// `finish_text` plus the cleaned text before the self-correction pass when
/// that pass changed it, so History can show what was dropped.
fn finish_parts(h: &PusmogoEngine, raw: &str, lang: Language) -> (String, Option<String>) {
    let mut cleaned = clean(&crate::context::respell(raw, &h.context), &h.dict);
    if lang == Language::Hungarian {
        let hinted = |word: &str| h.context.iter().any(|c| c.split_whitespace().any(|c| c == word));
        cleaned = crate::spell::fix(&cleaned, |word| h.dict.has_word(word) || hinted(word));
    }
    if !h.self_corrections {
        return (cleaned, None);
    }
    let typed = crate::backtrack::drop_self_corrections(&cleaned);
    if typed == cleaned {
        (typed, None)
    } else {
        (typed, Some(cleaned))
    }
}

/// Saves the dictation in the debug folder when one is set. Failures are
/// ignored: the record is a diagnostic and must never cost the user a dictation.
fn keep_debug(h: &PusmogoEngine, samples: &[f32], lang: Language, raw: &str, cleaned: &str, prompt: &str) {
    if let Some(dir) = &h.debug_dir {
        let lang = lang.whisper_code().unwrap_or("auto");
        let record = crate::debug::Record { lang, raw, cleaned, prompt, hints: &h.context };
        let _ = crate::debug::keep(dir, samples, &record);
    }
}

/// Runs the body of a C entry point and turns a panic into the function's
/// usual error value (NULL, -1, or a safe default), so a bug in the core
/// costs one result instead of aborting the app mid-dictation: unwinding out
/// of an `extern "C"` function aborts. The app already falls back on these
/// values (it types the raw text when cleanup returns NULL). Needs
/// `panic = "unwind"` in the release profile (`core/Cargo.toml`).
fn guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).unwrap_or(fallback)
}

/// A string argument that makes `str_arg` panic in tests, to prove every
/// entry point that reads one returns its error value instead of aborting.
#[cfg(test)]
const PANIC_PROBE: &str = "\u{1}panic-probe";

unsafe fn str_arg<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        None
    } else {
        let s = CStr::from_ptr(p).to_str().ok();
        #[cfg(test)]
        assert!(s != Some(PANIC_PROBE), "panic probe");
        s
    }
}

fn into_c(s: String) -> *mut c_char {
    // Interior NULs cannot come from an engine in practice; strip them to be safe.
    CString::new(s.replace('\0', "")).map(CString::into_raw).unwrap_or(ptr::null_mut())
}

/// Creates an engine. `model_path` may be NULL or missing on disk: the stub engine is used.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_engine_new(model_path: *const c_char) -> *mut PusmogoEngine {
    guard(ptr::null_mut(), || {
        match engine::load(str_arg(model_path)) {
            Ok(engine) => Box::into_raw(Box::new(PusmogoEngine { engine, dict: Dictionary::new(), context: Vec::new(), debug_dir: None, self_corrections: true, last_spoken: None })),
            Err(_) => ptr::null_mut(),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn pusmogo_engine_free(handle: *mut PusmogoEngine) {
    guard((), || {
        if !handle.is_null() {
            drop(Box::from_raw(handle));
            crate::spell::unload();
        }
    })
}

/// Name of the active engine ("whisper" or "stub"). Free with `pusmogo_string_free`.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_engine_name(handle: *const PusmogoEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match handle.as_ref() {
            Some(h) => into_c(h.engine.name().to_string()),
            None => ptr::null_mut(),
        }
    })
}

/// 1 when a speech engine was compiled in (it still needs its model file), 0 in
/// a stub-only build. Lets the app refuse to dictate while the model is missing
/// instead of typing the stub's placeholder text.
#[no_mangle]
pub extern "C" fn pusmogo_has_speech_engine() -> i32 {
    i32::from(cfg!(feature = "whisper"))
}

/// The model `language` should load: 0 turbo, 1 large-v3, 2 Pusmogo HU, -1 when
/// none is installed. `preference` is 0 auto, 1 fast, 2 most accurate, 3 Pusmogo
/// HU; `physical_memory` in bytes. Falls back to an installed model when the
/// wanted one is not on disk.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_model_pick(
    language: *const c_char,
    preference: i32,
    physical_memory: u64,
    turbo_installed: i32,
    large_installed: i32,
    hu_installed: i32,
) -> i32 {
    guard(-1, || {
        let lang = str_arg(language).unwrap_or("hu");
        let pref = models::Preference::from_i32(preference);
        let installed = |m: models::SpeechModel| match m {
            models::SpeechModel::Turbo => turbo_installed != 0,
            models::SpeechModel::LargeV3 => large_installed != 0,
            models::SpeechModel::PusmogoHu => hu_installed != 0,
        };
        models::pick(lang, pref, physical_memory, installed).map_or(-1, |m| m as i32)
    })
}

/// The model `language` should use when every model is on disk: 0 turbo, 1 large-v3, 2 Pusmogo HU.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_model_wanted(language: *const c_char, preference: i32, physical_memory: u64) -> i32 {
    guard(0, || {
        let lang = str_arg(language).unwrap_or("hu");
        models::wanted(lang, models::Preference::from_i32(preference), physical_memory) as i32
    })
}

/// Adds a dictionary entry (`from` misheard form → `to` wanted spelling) to this
/// engine only, not to the profile. Integration API: the app loads its dictionary
/// from the profile instead; kept for hosts that have no profile.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_dictionary_add(handle: *mut PusmogoEngine, from: *const c_char, to: *const c_char) {
    guard((), || {
        if let (Some(h), Some(from), Some(to)) = (handle.as_mut(), str_arg(from), str_arg(to)) {
            h.dict.add(from, to);
        }
    })
}

/// How many of the oldest dictionary spellings no longer fit Whisper's prompt
/// window (their replacements still apply). -1 when unknown: no handle, or an
/// engine without a tokenizer.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_dictionary_prompt_left_out(handle: *const PusmogoEngine) -> i32 {
    guard(-1, || {
        let Some(h) = handle.as_ref() else { return -1 };
        h.dict
            .prompt_left_out(crate::engine::MAX_PROMPT_TOKENS, |s| h.engine.count_tokens(s))
            .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX))
    })
}

/// Transcribes 16 kHz mono f32 samples and returns cleaned text.
/// `lang` is "hu", "en" or NULL/"auto". Returns NULL on error.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_transcribe(
    handle: *mut PusmogoEngine,
    samples: *const f32,
    len: usize,
    lang: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(h) = handle.as_mut() else { return ptr::null_mut() };
        if samples.is_null() || len == 0 {
            return into_c(String::new());
        }
        let audio = std::slice::from_raw_parts(samples, len);
        let lang = str_arg(lang).map(Language::from_code).unwrap_or(Language::Auto);
        let prompt = dict_prompt(h, lang, audio);
        h.last_spoken = None;
        match crate::session::transcribe_all(h.engine.as_mut(), audio, lang, &prompt) {
            Ok(raw) => {
                let (cleaned, spoken) = finish_parts(h, &raw, lang);
                h.last_spoken = spoken;
                keep_debug(h, audio, lang, &raw, &cleaned, &prompt);
                into_c(cleaned)
            }
            Err(_) => ptr::null_mut(),
        }
    })
}

/// Turns on the local record of recent dictations: each finished dictation
/// leaves its 16 kHz audio and the engine's output in `dir` (the newest 20 are
/// kept, see `debug`). NULL or "" turns it off. Returns 0, or -1 on a NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_set_debug_dir(handle: *mut PusmogoEngine, dir: *const c_char) -> i32 {
    guard(-1, || {
        let Some(h) = handle.as_mut() else { return -1 };
        h.debug_dir = str_arg(dir).filter(|d| !d.is_empty()).map(std::path::PathBuf::from);
        0
    })
}

/// Turns dropping spoken self-corrections ("háromkor, nem, négykor" →
/// "négykor", see `backtrack`) on (1, the default) or off (0) for this
/// engine's dictations and captions. Returns 0, or -1 on a NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_set_self_corrections(handle: *mut PusmogoEngine, on: i32) -> i32 {
    guard(-1, || {
        let Some(h) = handle.as_mut() else { return -1 };
        h.self_corrections = on != 0;
        0
    })
}

/// The last dictation (`pusmogo_transcribe` or `pusmogo_session_finish`) as
/// cleaned before its spoken self-corrections were dropped, for History to
/// show what was left out. NULL when nothing was dropped or on a NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_last_spoken(handle: *const PusmogoEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match handle.as_ref().and_then(|h| h.last_spoken.clone()) {
            Some(s) => into_c(s),
            None => ptr::null_mut(),
        }
    })
}

/// The dictionary prompt plus the context hints (when `samples` are long
/// enough for them, see `context::prompt_hints`), labelled in `lang` and
/// trimmed to what Whisper keeps (224 tokens).
fn dict_prompt(h: &PusmogoEngine, lang: Language, samples: &[f32]) -> String {
    let hints = crate::context::prompt_hints(&h.context, samples);
    h.dict.prompt_fitting_with(hints, lang, crate::engine::MAX_PROMPT_TOKENS, |s| h.engine.count_tokens(s))
}

/// Sets the text around the cursor in the focused field (the app reads it
/// just before a dictation). Name-like words from it steer the engine's
/// spelling until the next call; NULL or "" clears them. Returns how many hint
/// words were kept, or -1 on a NULL handle. The text itself is not kept.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_set_context(handle: *mut PusmogoEngine, text: *const c_char) -> i32 {
    guard(-1, || {
        let Some(h) = handle.as_mut() else { return -1 };
        h.context = str_arg(text).map_or_else(Vec::new, |t| crate::context::hint_words(t, crate::context::MAX_HINT_WORDS));
        i32::try_from(h.context.len()).unwrap_or(i32::MAX)
    })
}

/// `pusmogo_set_context` for a code editor or terminal: identifiers in snake
/// or constant case and file names with a known extension count as hints too
/// (`context::code_hint_words`). The app adds the window title, which names
/// the open file in most editors.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_set_code_context(handle: *mut PusmogoEngine, text: *const c_char) -> i32 {
    guard(-1, || {
        let Some(h) = handle.as_mut() else { return -1 };
        h.context = str_arg(text).map_or_else(Vec::new, |t| crate::context::code_hint_words(t, crate::context::MAX_HINT_WORDS));
        i32::try_from(h.context.len()).unwrap_or(i32::MAX)
    })
}

/// Set by `pusmogo_session_interrupt` from any thread: a caption preview in
/// progress gives up, and so does a chunk pass the final pass would redo, so
/// the final pass does not wait behind either.
static RELEASED: AtomicBool = AtomicBool::new(false);

/// Starts a streaming dictation (see `session`). `lang` as in `pusmogo_transcribe`.
/// Free with `pusmogo_session_free`.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_session_new(lang: *const c_char) -> *mut Session {
    guard(ptr::null_mut(), || {
        RELEASED.store(false, Ordering::Relaxed);
        let lang = str_arg(lang).map(Language::from_code).unwrap_or(Language::Auto);
        Box::into_raw(Box::new(Session::new(lang)))
    })
}

#[no_mangle]
pub unsafe extern "C" fn pusmogo_session_free(session: *mut Session) {
    guard((), || {
        if !session.is_null() {
            drop(Box::from_raw(session));
        }
    })
}

/// Appends 16 kHz mono f32 samples to the session.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_session_push(session: *mut Session, samples: *const f32, len: usize) {
    guard((), || {
        if let (Some(s), false) = (session.as_mut(), samples.is_null() || len == 0) {
            s.push(std::slice::from_raw_parts(samples, len));
        }
    })
}

/// Transcribes one finished chunk if one is ready, else (with previews on,
/// see `pusmogo_session_set_previews`) makes a caption preview of the waiting
/// audio when one is due. Call it about once a second while recording. Returns 1 if the caption text changed, 0 if not, -1 on
/// error or a NULL argument.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_session_step(handle: *mut PusmogoEngine, session: *mut Session) -> i32 {
    guard(-1, || {
        let (Some(h), Some(s)) = (handle.as_mut(), session.as_mut()) else { return -1 };
        // The dictation record saves the whole audio; otherwise the session drops
        // audio no pass will read again, and hints are judged on what is kept
        // (always at least the audio the final pass decodes).
        s.keep_all = h.debug_dir.is_some();
        let prompt = dict_prompt(h, s.lang(), s.samples());
        let stepped = match s.step_until(h.engine.as_mut(), &prompt, &RELEASED) {
            Ok(true) => return 1,
            Ok(false) if s.previews => s.preview(h.engine.as_mut(), &prompt, &RELEASED),
            Ok(false) => Ok(false),
            Err(e) => Err(e),
        };
        match stepped {
            Ok(true) => 1,
            Ok(false) => 0,
            Err(_) => -1,
        }
    })
}

/// Turns caption previews on (1) or off (0, the default) for this session.
/// Off by default: on Metal a preview cannot stop midway, so a key release
/// during one waits up to ~0.6 s for it (docs/bench/caption-01.md).
#[no_mangle]
pub unsafe extern "C" fn pusmogo_session_set_previews(session: *mut Session, on: i32) {
    guard((), || {
        if let Some(s) = session.as_mut() {
            s.previews = on != 0;
        }
    })
}

/// Makes a caption preview in progress give up at once and keeps new ones
/// from starting until the next `pusmogo_session_new` (call it on key release,
/// from any thread, before queueing `pusmogo_session_finish`). A chunk pass in
/// progress gives up too when the final pass would redo its audio anyway.
/// Previews never add to the final text, so nothing is lost.
#[no_mangle]
pub extern "C" fn pusmogo_session_interrupt() {
    RELEASED.store(true, Ordering::Relaxed);
}

/// Cleaned text for the live caption: the chunks transcribed so far plus the
/// latest preview of the audio after them (empty until the first preview).
/// It may change at `pusmogo_session_finish`, which redoes the last chunks
/// with the tail. NULL on a NULL argument.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_session_text(handle: *const PusmogoEngine, session: *const Session) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(h), Some(s)) = (handle.as_ref(), session.as_ref()) else { return ptr::null_mut() };
        into_c(finish_text(h, &s.caption(), s.lang()))
    })
}

/// Transcribes the rest of the session and returns the cleaned text of the
/// whole dictation. NULL on error.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_session_finish(handle: *mut PusmogoEngine, session: *mut Session) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(h), Some(s)) = (handle.as_mut(), session.as_mut()) else { return ptr::null_mut() };
        s.keep_all = h.debug_dir.is_some();
        let prompt = dict_prompt(h, s.lang(), s.samples());
        h.last_spoken = None;
        match s.finish(h.engine.as_mut(), &prompt) {
            Ok(raw) => {
                let (cleaned, spoken) = finish_parts(h, &raw, s.lang());
                h.last_spoken = spoken;
                keep_debug(h, s.samples(), s.lang(), &raw, &cleaned, &prompt);
                into_c(cleaned)
            }
            Err(_) => ptr::null_mut(),
        }
    })
}

/// Runs the model once on synthetic audio so the first real dictation is not
/// slowed by one-time setup. Returns 0 on success, -1 on error or a NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_engine_warm_up(handle: *mut PusmogoEngine) -> i32 {
    guard(-1, || {
        match handle.as_mut().map(|h| h.engine.warm_up()) {
            Some(Ok(())) => 0,
            _ => -1,
        }
    })
}

/// Runs only the text cleanup, without an engine or a dictionary. Integration API:
/// not called by the app; kept for hosts and tests that clean text they already have.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_clean_text(text: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match str_arg(text) {
            Some(t) => into_c(clean(t, &Dictionary::new())),
            None => ptr::null_mut(),
        }
    })
}

/// Joins the lines of `text` with single spaces, for single-line fields.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_single_line(text: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match str_arg(text) {
            Some(t) => into_c(crate::cleanup::single_line(t)),
            None => ptr::null_mut(),
        }
    })
}

/// The writing-style category of the frontmost app (`style.rs`): 0 other,
/// 1 personal messages, 2 work messages, 3 email, 4 code (editors, IDEs,
/// terminals). `window_title` matters only
/// for browsers (web mail and chat); `overrides` is the user's mapping, one
/// `bundle.id<TAB>category` per line. Any argument may be NULL.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_style_category(bundle_id: *const c_char, window_title: *const c_char, overrides: *const c_char) -> i32 {
    guard(0, || {
        let arg = |p| str_arg(p).unwrap_or("");
        crate::style::category_for(arg(bundle_id), arg(window_title), arg(overrides)) as i32
    })
}

/// Applies a writing style to already cleaned text: `category` as returned by
/// `pusmogo_style_category`, `style` 0 formal, 1 casual, 2 very casual,
/// `lang` "hu" or "en". Email also lays out a spoken greeting and sign-off;
/// code ignores `style` (no capital or final stop, straight quotes, spoken
/// case commands such as "camel case user id" → `userId`).
#[no_mangle]
pub unsafe extern "C" fn pusmogo_style_apply(text: *const c_char, category: i32, style: i32, lang: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match str_arg(text) {
            Some(t) => into_c(crate::style::apply(
                t,
                crate::style::Category::from_i32(category),
                crate::style::Style::from_i32(style),
                Language::from_code(str_arg(lang).unwrap_or("")),
            )),
            None => ptr::null_mut(),
        }
    })
}

/// After an insertion, compares the field right after it (`before`) with the
/// field a few seconds later (`after`). If the user replaced up to three words
/// of the dictated text `dictated`, returns "wrong\tright" for an "Add to
/// dictionary?" offer; NULL otherwise (see `edits.rs`).
#[no_mangle]
pub unsafe extern "C" fn pusmogo_learned_correction(
    dictated: *const c_char,
    before: *const c_char,
    after: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match (str_arg(dictated), str_arg(before), str_arg(after)) {
            (Some(d), Some(b), Some(a)) => match crate::edits::learned_correction(d, b, a) {
                Some(c) => into_c(format!("{}\t{}", c.wrong, c.right)),
                None => ptr::null_mut(),
            },
            _ => ptr::null_mut(),
        }
    })
}

// Profile: history, dictionary, settings and snippets in one SQLite file (see `profile.rs`).
// Functions returning `i32` give 0 on success and -1 on error.

fn status<T, E>(r: Result<T, E>) -> i32 {
    if r.is_ok() {
        0
    } else {
        -1
    }
}

/// Opens or creates the profile file at `path` (NULL on error, e.g. a newer schema).
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_open(path: *const c_char) -> *mut Profile {
    guard(ptr::null_mut(), || {
        match str_arg(path).map(Profile::open) {
            Some(Ok(p)) => Box::into_raw(Box::new(p)),
            _ => ptr::null_mut(),
        }
    })
}

/// Like `pusmogo_profile_open`, but a file that is not an SQLite database or
/// is damaged is moved aside and a fresh profile is started. When that
/// happened, `*moved_to` (if `moved_to` is not NULL) receives the new path of
/// the old file (free with `pusmogo_string_free`); otherwise it is set to
/// NULL. A profile from a newer version is never moved: NULL is returned.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_open_recovering(path: *const c_char, moved_to: *mut *mut c_char) -> *mut Profile {
    guard(ptr::null_mut(), || {
        if !moved_to.is_null() {
            *moved_to = ptr::null_mut();
        }
        match str_arg(path).map(Profile::open_or_recover) {
            Some(Ok((p, aside))) => {
                if let (Some(aside), false) = (aside, moved_to.is_null()) {
                    *moved_to = into_c(aside.to_string_lossy().into_owned());
                }
                Box::into_raw(Box::new(p))
            }
            _ => ptr::null_mut(),
        }
    })
}

/// 1 when the file at `path` is a profile written by a newer Pusmogo (the
/// reason `pusmogo_profile_open_recovering` returned NULL), 0 otherwise
/// (missing, damaged, current, or `path` NULL). Reads only.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_is_newer(path: *const c_char) -> i32 {
    guard(0, || {
        str_arg(path).map_or(0, |p| i32::from(Profile::is_from_newer_version(p)))
    })
}

#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_free(profile: *mut Profile) {
    guard((), || {
        if !profile.is_null() {
            drop(Box::from_raw(profile));
        }
    })
}

/// Stores one dictation; returns its id, or -1 on error. `app` may be NULL.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_add_history(
    profile: *mut Profile,
    lang: *const c_char,
    text: *const c_char,
    latency_ms: i64,
    app: *const c_char,
) -> i64 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(lang), str_arg(text)) {
            (Some(p), Some(lang), Some(text)) => p.add_history(lang, text, latency_ms, str_arg(app).unwrap_or("")).unwrap_or(-1),
            _ => -1,
        }
    })
}

/// Newest first, as a JSON array of `{id, created_at, lang, text, latency_ms, app}`.
/// `query` NULL or "" lists everything.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_search_history(profile: *mut Profile, query: *const c_char, limit: usize) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match profile.as_ref().map(|p| p.search_history(str_arg(query).unwrap_or(""), limit)) {
            Some(Ok(entries)) => into_c(profile::history_json(&entries)),
            _ => ptr::null_mut(),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_delete_history(profile: *mut Profile, id: i64) -> i32 {
    guard(-1, || {
        profile.as_ref().map_or(-1, |p| status(p.delete_history(id)))
    })
}

#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_clear_history(profile: *mut Profile) -> i32 {
    guard(-1, || {
        profile.as_ref().map_or(-1, |p| status(p.clear_history()))
    })
}

/// Drops history older than `keep_days` (0 keeps all). Returns rows removed, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_prune_history(profile: *mut Profile, keep_days: i64) -> i64 {
    guard(-1, || {
        profile.as_ref().and_then(|p| p.prune_history(keep_days).ok()).map_or(-1, |n| n as i64)
    })
}

/// `{"dictations": n, "words": n, "median_latency_ms": n}`. Integration API: not
/// called by the app yet; kept for hosts that show usage statistics.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_stats(profile: *mut Profile) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match profile.as_ref().map(Profile::stats) {
            Some(Ok(s)) => into_c(format!(
                "{{\"dictations\":{},\"words\":{},\"median_latency_ms\":{}}}",
                s.dictations, s.words, s.median_latency_ms
            )),
            _ => ptr::null_mut(),
        }
    })
}

/// Stores the text of history entry `id` as spoken, before its self-corrections
/// were dropped (`pusmogo_last_spoken`). Returns 0, or -1 on error.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_set_spoken(profile: *mut Profile, id: i64, spoken: *const c_char) -> i32 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(spoken)) {
            (Some(p), Some(spoken)) => status(p.set_history_spoken(id, spoken)),
            _ => -1,
        }
    })
}

/// Stores how long the recording behind history entry `id` was, for the
/// words-per-minute insight. Returns 0, or -1 on error.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_set_audio_ms(profile: *mut Profile, id: i64, audio_ms: i64) -> i32 {
    guard(-1, || {
        profile.as_ref().map_or(-1, |p| status(p.set_history_audio_ms(id, audio_ms)))
    })
}

/// Records that the user corrected `wrong` to `right` (an accepted dictionary
/// offer), for the "most corrected word" insight. Returns 0, or -1 on error.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_add_correction(profile: *mut Profile, wrong: *const c_char, right: *const c_char) -> i32 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(wrong), str_arg(right)) {
            (Some(p), Some(w), Some(r)) if !w.trim().is_empty() && !r.trim().is_empty() => status(p.add_correction(w, r)),
            _ => -1,
        }
    })
}

/// Records that the user fixed `wrong` to `right` after a dictation (seen by
/// the edit watch, not accepted). Returns 1 when the same fix was seen before
/// and `right` was just added to the dictionary without asking, 0 when it was
/// only recorded (or the dictionary already makes it), -1 on error.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_note_correction(profile: *mut Profile, wrong: *const c_char, right: *const c_char) -> i32 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(wrong), str_arg(right)) {
            (Some(p), Some(w), Some(r)) => match p.note_correction(w, r) {
                Ok(learned) => learned as i32,
                Err(_) => -1,
            },
            _ => -1,
        }
    })
}

/// Usage insights over the whole history as JSON (see `insights::to_json`).
/// Days and hours follow the system time zone at each dictation's own date
/// (summer time included); `utc_offset_s`, the current local offset from UTC,
/// is only a fallback where that conversion fails.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_insights(profile: *mut Profile, utc_offset_s: i64) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match profile.as_ref().map(|p| p.insights_now(utc_offset_s)) {
            Some(Ok(i)) => into_c(crate::insights::to_json(&i)),
            _ => ptr::null_mut(),
        }
    })
}

/// Adds or updates a dictionary entry (`from` is case-insensitive).
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_set_dictionary(profile: *mut Profile, from: *const c_char, to: *const c_char) -> i32 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(from), str_arg(to)) {
            (Some(p), Some(from), Some(to)) => status(p.set_dictionary_entry(from, to)),
            _ => -1,
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_remove_dictionary(profile: *mut Profile, from: *const c_char) -> i32 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(from)) {
            (Some(p), Some(from)) => status(p.remove_dictionary_entry(from)),
            _ => -1,
        }
    })
}

/// JSON array of `{from, to}` in the order added.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_dictionary(profile: *mut Profile) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match profile.as_ref().map(Profile::dictionary_entries) {
            Some(Ok(entries)) => into_c(profile::dictionary_json(&entries)),
            _ => ptr::null_mut(),
        }
    })
}

/// Replaces the engine's dictionary with the profile's. Call after opening an
/// engine (including a reload after idle) and after every dictionary edit.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_apply_dictionary(profile: *mut Profile, handle: *mut PusmogoEngine) -> i32 {
    guard(-1, || {
        match (profile.as_ref(), handle.as_mut()) {
            (Some(p), Some(h)) => match p.dictionary() {
                Ok(d) => {
                    h.dict = d;
                    0
                }
                Err(_) => -1,
            },
            _ => -1,
        }
    })
}

/// Adds a snippet or replaces the one with the same trigger (compared without
/// case, accents or punctuation). Returns its id, or -1 (empty trigger or text).
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_set_snippet(profile: *mut Profile, trigger: *const c_char, text: *const c_char) -> i64 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(trigger), str_arg(text)) {
            (Some(p), Some(trigger), Some(text)) => p.set_snippet(trigger, text).unwrap_or(-1),
            _ => -1,
        }
    })
}

/// Changes snippet `id`; -1 if the trigger is empty or belongs to another snippet.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_update_snippet(
    profile: *mut Profile,
    id: i64,
    trigger: *const c_char,
    text: *const c_char,
) -> i32 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(trigger), str_arg(text)) {
            (Some(p), Some(trigger), Some(text)) => status(p.update_snippet(id, trigger, text)),
            _ => -1,
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_delete_snippet(profile: *mut Profile, id: i64) -> i32 {
    guard(-1, || {
        profile.as_ref().map_or(-1, |p| status(p.delete_snippet(id)))
    })
}

/// JSON array of `{id, trigger, text, created_at}` in the order added.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_snippets(profile: *mut Profile) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match profile.as_ref().map(Profile::snippets) {
            Some(Ok(entries)) => into_c(profile::snippets_json(&entries)),
            _ => ptr::null_mut(),
        }
    })
}

/// The text to insert verbatim when the whole cleaned `utterance` is a snippet
/// trigger; NULL otherwise (insert the dictation as usual).
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_snippet_for(profile: *mut Profile, utterance: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match (profile.as_ref(), str_arg(utterance)) {
            (Some(p), Some(u)) => p.snippet_for(u).ok().flatten().map_or(ptr::null_mut(), into_c),
            _ => ptr::null_mut(),
        }
    })
}

/// Value for `key`, or NULL if unset.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_setting(profile: *mut Profile, key: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match (profile.as_ref(), str_arg(key)) {
            (Some(p), Some(key)) => p.setting(key).ok().flatten().map_or(ptr::null_mut(), into_c),
            _ => ptr::null_mut(),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_set_setting(profile: *mut Profile, key: *const c_char, value: *const c_char) -> i32 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(key), str_arg(value)) {
            (Some(p), Some(key), Some(value)) => status(p.set_setting(key, value)),
            _ => -1,
        }
    })
}

/// Writes a copy of the whole profile to `path` (which must not exist yet).
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_export(profile: *mut Profile, path: *const c_char) -> i32 {
    guard(-1, || {
        match (profile.as_ref(), str_arg(path)) {
            (Some(p), Some(path)) => status(p.export_to(path)),
            _ => -1,
        }
    })
}

/// Replaces the profile's contents with an exported file; on error nothing changes.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_profile_import(profile: *mut Profile, path: *const c_char) -> i32 {
    guard(-1, || {
        match (profile.as_mut(), str_arg(path)) {
            (Some(p), Some(path)) => status(p.import_from(path)),
            _ => -1,
        }
    })
}

fn testset_takes() -> &'static [crate::testset::Take] {
    static TAKES: std::sync::OnceLock<Vec<crate::testset::Take>> = std::sync::OnceLock::new();
    TAKES.get_or_init(crate::testset::builtin_plan)
}

/// Test-set recording: the folder for speaker `name` under `root` (letters,
/// digits, `-`, `_` only). NULL when an argument is NULL.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_testset_speaker_dir(root: *const c_char, name: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match (str_arg(root), str_arg(name)) {
            (Some(root), Some(name)) => into_c(crate::testset::speaker_dir(std::path::Path::new(root), name).to_string_lossy().into_owned()),
            _ => ptr::null_mut(),
        }
    })
}

/// The built-in reading script's takes and what `dir` already holds, as JSON
/// (`testset::status_json`). NULL when `dir` is NULL.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_testset_status(dir: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match str_arg(dir) {
            Some(dir) => into_c(crate::testset::status_json(std::path::Path::new(dir), testset_takes())),
            None => ptr::null_mut(),
        }
    })
}

/// Saves one take (16 kHz mono f32) as `<id>.wav` / `<id>-whisper.wav` plus its
/// reference text. 0 saved, -1 bad argument or unknown take, -2 shorter than
/// 0.3 s, -3 could not write.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_testset_save(dir: *const c_char, id: *const c_char, whispered: i32, samples: *const f32, len: usize) -> i32 {
    guard(-1, || {
        use crate::testset::{save_take, Error};
        let (Some(dir), Some(id)) = (str_arg(dir), str_arg(id)) else { return -1 };
        if samples.is_null() {
            return -1;
        }
        let audio = std::slice::from_raw_parts(samples, len);
        match save_take(std::path::Path::new(dir), testset_takes(), id, whispered != 0, audio) {
            Ok(_) => 0,
            Err(Error::TooShort) => -2,
            Err(Error::Io(_)) => -3,
            Err(_) => -1,
        }
    })
}

/// Writes `consent.txt` in `dir`: `level` is "private" or "release", `date`
/// e.g. "2026-11-09", `who` initials or a pseudonym (may be empty). 0 or -1.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_testset_consent(dir: *const c_char, level: *const c_char, date: *const c_char, who: *const c_char) -> i32 {
    guard(-1, || {
        match (str_arg(dir), str_arg(level), str_arg(date)) {
            (Some(dir), Some(level), Some(date)) => {
                status(crate::testset::write_consent(std::path::Path::new(dir), level, date, str_arg(who).unwrap_or("")))
            }
            _ => -1,
        }
    })
}

// MCP config for local AI apps (mcp_config.rs). `home` is the user's home
// folder, passed in so tests and self-tests can use a scratch folder;
// `client` is "claude-desktop", "claude-code" or "cursor".

fn mcp_args(home: *const c_char, client: *const c_char) -> Option<(std::path::PathBuf, crate::mcp_config::Client)> {
    // SAFETY: callers pass NULL or valid NUL-terminated strings (see pusmogo.h).
    let (home, client) = unsafe { (str_arg(home)?, str_arg(client)?) };
    Some((std::path::PathBuf::from(home), crate::mcp_config::Client::from_id(client)?))
}

fn mcp_code(r: Result<bool, crate::mcp_config::ConfigError>) -> i32 {
    use crate::mcp_config::ConfigError;
    match r {
        Ok(changed) => i32::from(changed),
        Err(ConfigError::Unreadable(_) | ConfigError::UnexpectedShape(_)) => -2,
        Err(ConfigError::Io(_)) => -3,
    }
}

/// Path of `client`'s config file under `home`. NULL on a bad argument.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_mcp_config_path(home: *const c_char, client: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        match mcp_args(home, client) {
            Some((home, client)) => into_c(client.config_path(&home).to_string_lossy().into_owned()),
            None => ptr::null_mut(),
        }
    })
}

/// 1 when `client`'s config holds the Pusmogo server starting `command`, 2 when
/// it holds one starting another path (a moved or deleted copy), 0 when not (or
/// no file), -1 bad argument, -2 the file is not a JSON object we can read,
/// -3 read failed.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_mcp_installed(home: *const c_char, client: *const c_char, command: *const c_char) -> i32 {
    guard(-1, || {
        use crate::mcp_config::Registration;
        let (Some((home, client)), Some(command)) = (mcp_args(home, client), str_arg(command)) else { return -1 };
        match crate::mcp_config::installed(&home, client, command) {
            Ok(Registration::Missing) => 0,
            Ok(Registration::Current) => 1,
            Ok(Registration::OtherCommand) => 2,
            Err(e) => mcp_code(Err(e)),
        }
    })
}

/// The unified diff installing would apply, "" when the entry is already
/// there, NULL on a bad argument or a file we will not touch.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_mcp_preview(home: *const c_char, client: *const c_char, command: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some((home, client)), Some(command)) = (mcp_args(home, client), str_arg(command)) else { return ptr::null_mut() };
        match crate::mcp_config::preview_install(&home, client, command) {
            Ok(diff) => into_c(diff.unwrap_or_default()),
            Err(_) => ptr::null_mut(),
        }
    })
}

/// Adds the server (`command` = path of the bundled pusmogo-mcp) after backing
/// the file up. 1 changed, 0 already there, -1 bad argument, -2 unreadable
/// file left untouched, -3 write failed.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_mcp_install(home: *const c_char, client: *const c_char, command: *const c_char) -> i32 {
    guard(-1, || {
        match (mcp_args(home, client), str_arg(command)) {
            (Some((home, client)), Some(command)) => mcp_code(crate::mcp_config::install(&home, client, command)),
            _ => -1,
        }
    })
}

/// Undo: removes only the Pusmogo entry, after a backup. Codes as install.
#[no_mangle]
pub unsafe extern "C" fn pusmogo_mcp_uninstall(home: *const c_char, client: *const c_char) -> i32 {
    guard(-1, || {
        match mcp_args(home, client) {
            Some((home, client)) => mcp_code(crate::mcp_config::uninstall(&home, client)),
            None => -1,
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn pusmogo_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_corrections_are_dropped_unless_turned_off() {
        let h = unsafe { pusmogo_engine_new(ptr::null()) };
        assert!(!h.is_null());
        let said = "Holnap háromkor, nem, négykor találkozunk.";
        unsafe {
            assert_eq!(finish_text(&*h, said, Language::Hungarian), "Holnap négykor találkozunk.", "on by default");
            assert_eq!(pusmogo_set_self_corrections(h, 0), 0);
            assert_eq!(finish_text(&*h, said, Language::Hungarian), said);
            assert_eq!(pusmogo_set_self_corrections(h, 1), 0);
            assert_eq!(finish_text(&*h, "Ez nem kettő, hanem három.", Language::Hungarian), "Ez nem kettő, hanem három.");
            assert_eq!(pusmogo_set_self_corrections(ptr::null_mut(), 1), -1);
            pusmogo_engine_free(h);
        }
    }

    #[test]
    fn the_text_before_self_corrections_is_kept_for_history() {
        let h = unsafe { pusmogo_engine_new(ptr::null()) };
        assert!(!h.is_null());
        unsafe {
            let said = "Holnap háromkor, nem, négykor találkozunk.";
            assert_eq!(finish_parts(&*h, said, Language::Hungarian), ("Holnap négykor találkozunk.".to_string(), Some(said.to_string())));
            assert_eq!(finish_parts(&*h, "Ez nem kettő, hanem három.", Language::Hungarian).1, None, "nothing dropped");
            pusmogo_set_self_corrections(h, 0);
            assert_eq!(finish_parts(&*h, said, Language::Hungarian).1, None, "pass off");
            // The stub engine's dictation drops nothing: no spoken text.
            let audio = vec![0.0f32; 16_000];
            let hu = CString::new("hu").unwrap();
            let out = pusmogo_transcribe(h, audio.as_ptr(), audio.len(), hu.as_ptr());
            assert!(!out.is_null());
            pusmogo_string_free(out);
            assert!(pusmogo_last_spoken(h).is_null());
            assert!(pusmogo_last_spoken(ptr::null()).is_null());
            pusmogo_engine_free(h);
        }
    }

    #[test]
    fn testset_save_status_and_consent_over_the_c_abi() {
        let root = std::env::temp_dir().join(format!("pusmogo-ffi-testset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let c = |s: &str| CString::new(s).unwrap();
        unsafe {
            let dir_p = pusmogo_testset_speaker_dir(c(root.to_str().unwrap()).as_ptr(), c("Éva K").as_ptr());
            let dir = CStr::from_ptr(dir_p).to_str().unwrap().to_string();
            pusmogo_string_free(dir_p);
            assert!(dir.ends_with("/Eva-K"), "{dir}");
            let d = c(&dir);
            let audio = vec![0.05f32; 8000];
            assert_eq!(pusmogo_testset_save(d.as_ptr(), c("ts001").as_ptr(), 0, audio.as_ptr(), audio.len()), 0);
            assert_eq!(pusmogo_testset_save(d.as_ptr(), c("ts001").as_ptr(), 0, audio.as_ptr(), 100), -2);
            assert_eq!(pusmogo_testset_save(d.as_ptr(), c("zz999").as_ptr(), 0, audio.as_ptr(), audio.len()), -1);
            assert_eq!(pusmogo_testset_consent(d.as_ptr(), c("release").as_ptr(), c("2026-11-09").as_ptr(), ptr::null()), 0);
            assert_eq!(pusmogo_testset_consent(d.as_ptr(), c("yes").as_ptr(), c("2026-11-09").as_ptr(), ptr::null()), -1);
            let json_p = pusmogo_testset_status(d.as_ptr());
            let json = CStr::from_ptr(json_p).to_str().unwrap().to_string();
            pusmogo_string_free(json_p);
            assert!(json.starts_with("{\"consent\":\"release\",\"next\":1,"), "{}", &json[..80]);
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mcp_config_install_preview_and_undo_over_the_c_abi() {
        let home = std::env::temp_dir().join(format!("pusmogo-ffi-mcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let c = |s: &str| CString::new(s).unwrap();
        let (h, cursor, cmd) = (c(home.to_str().unwrap()), c("cursor"), c("/tmp/pusmogo-mcp"));
        let take = |p: *mut c_char| {
            assert!(!p.is_null());
            let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_string();
            unsafe { pusmogo_string_free(p) };
            s
        };
        unsafe {
            assert!(take(pusmogo_mcp_config_path(h.as_ptr(), cursor.as_ptr())).ends_with("/.cursor/mcp.json"));
            assert!(pusmogo_mcp_config_path(h.as_ptr(), c("vim").as_ptr()).is_null());
            assert_eq!(pusmogo_mcp_installed(h.as_ptr(), cursor.as_ptr(), cmd.as_ptr()), 0);
            let diff = take(pusmogo_mcp_preview(h.as_ptr(), cursor.as_ptr(), cmd.as_ptr()));
            assert!(diff.lines().any(|l| l.starts_with('+') && l.contains("\"pusmogo\"")), "{diff}");
            assert_eq!(pusmogo_mcp_install(h.as_ptr(), cursor.as_ptr(), cmd.as_ptr()), 1);
            assert_eq!(pusmogo_mcp_install(h.as_ptr(), cursor.as_ptr(), cmd.as_ptr()), 0);
            assert_eq!(pusmogo_mcp_installed(h.as_ptr(), cursor.as_ptr(), cmd.as_ptr()), 1);
            assert_eq!(take(pusmogo_mcp_preview(h.as_ptr(), cursor.as_ptr(), cmd.as_ptr())), "");
            let moved = c("/elsewhere/pusmogo-mcp");
            assert_eq!(pusmogo_mcp_installed(h.as_ptr(), cursor.as_ptr(), moved.as_ptr()), 2);
            assert_eq!(pusmogo_mcp_installed(h.as_ptr(), cursor.as_ptr(), ptr::null()), -1);
            assert_eq!(pusmogo_mcp_uninstall(h.as_ptr(), cursor.as_ptr()), 1);
            assert_eq!(pusmogo_mcp_installed(h.as_ptr(), cursor.as_ptr(), cmd.as_ptr()), 0);
            assert_eq!(pusmogo_mcp_install(ptr::null(), cursor.as_ptr(), cmd.as_ptr()), -1);
            let desktop = c("claude-desktop");
            let path = take(pusmogo_mcp_config_path(h.as_ptr(), desktop.as_ptr()));
            std::fs::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
            std::fs::write(&path, "{ not json").unwrap();
            assert_eq!(pusmogo_mcp_install(h.as_ptr(), desktop.as_ptr(), cmd.as_ptr()), -2);
            assert!(pusmogo_mcp_preview(h.as_ptr(), desktop.as_ptr(), cmd.as_ptr()).is_null());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
        }
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn reports_compiled_engine() {
        assert_eq!(pusmogo_has_speech_engine() == 1, cfg!(feature = "whisper"));
    }

    #[test]
    fn model_pick_through_c_abi() {
        let hu = CString::new("hu").unwrap();
        let en = CString::new("en").unwrap();
        let gb: u64 = 1 << 30;
        unsafe {
            assert_eq!(pusmogo_model_pick(hu.as_ptr(), 0, 8 * gb, 1, 1, 1), 2);
            assert_eq!(pusmogo_model_pick(hu.as_ptr(), 0, 16 * gb, 1, 1, 0), 1);
            assert_eq!(pusmogo_model_pick(en.as_ptr(), 0, 16 * gb, 1, 1, 1), 0);
            assert_eq!(pusmogo_model_pick(en.as_ptr(), 0, 16 * gb, 0, 0, 1), -1);
            assert_eq!(pusmogo_model_pick(hu.as_ptr(), 0, 16 * gb, 1, 0, 0), 0);
            assert_eq!(pusmogo_model_pick(hu.as_ptr(), 0, 16 * gb, 0, 0, 0), -1);
            assert_eq!(pusmogo_model_wanted(hu.as_ptr(), 1, 16 * gb), 0);
            assert_eq!(pusmogo_model_wanted(hu.as_ptr(), 3, 16 * gb), 2);
            assert_eq!(pusmogo_model_wanted(ptr::null(), 0, 16 * gb), 2);
        }
    }

    #[test]
    fn learned_correction_through_c_abi() {
        let d = CString::new("Holnap a klózisz csapattal.").unwrap();
        let fixed = CString::new("Holnap a Clausis csapattal.").unwrap();
        unsafe {
            let s = pusmogo_learned_correction(d.as_ptr(), d.as_ptr(), fixed.as_ptr());
            assert_eq!(CStr::from_ptr(s).to_str().unwrap(), "klózisz\tClausis");
            pusmogo_string_free(s);
            assert!(pusmogo_learned_correction(d.as_ptr(), d.as_ptr(), d.as_ptr()).is_null());
            assert!(pusmogo_learned_correction(ptr::null(), d.as_ptr(), fixed.as_ptr()).is_null());
        }
    }

    #[test]
    fn profile_open_recovering_reports_the_moved_file() {
        let dir = std::env::temp_dir().join(format!("pusmogo-ffi-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("profile.sqlite");
        std::fs::write(&path, b"this is not a database, just text").unwrap();
        let c_path = CString::new(path.to_str().unwrap()).unwrap();
        unsafe {
            assert!(pusmogo_profile_open(c_path.as_ptr()).is_null());
            let mut moved: *mut c_char = ptr::null_mut();
            let p = pusmogo_profile_open_recovering(c_path.as_ptr(), &mut moved);
            assert!(!p.is_null());
            assert!(!moved.is_null());
            let aside = CStr::from_ptr(moved).to_str().unwrap().to_owned();
            pusmogo_string_free(moved);
            assert_eq!(std::fs::read(&aside).unwrap(), b"this is not a database, just text");
            pusmogo_profile_free(p);
            // Second open: healthy, nothing moved; a NULL out-pointer is fine.
            let mut moved: *mut c_char = ptr::dangling_mut();
            let p = pusmogo_profile_open_recovering(c_path.as_ptr(), &mut moved);
            assert!(!p.is_null() && moved.is_null());
            pusmogo_profile_free(p);
            let p = pusmogo_profile_open_recovering(c_path.as_ptr(), ptr::null_mut());
            assert!(!p.is_null());
            pusmogo_profile_free(p);
            assert!(pusmogo_profile_open_recovering(ptr::null(), ptr::null_mut()).is_null());
            assert_eq!(pusmogo_profile_is_newer(c_path.as_ptr()), 0);
            assert_eq!(pusmogo_profile_is_newer(ptr::null()), 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The integration calls the app itself does not use still work for other hosts.
    #[test]
    fn panic_in_the_core_returns_the_error_value() {
        let probe = CString::new(PANIC_PROBE).unwrap();
        let hu = CString::new("hu").unwrap();
        let pr = probe.as_ptr();
        unsafe {
            // Text entry points.
            assert!(pusmogo_clean_text(pr).is_null());
            assert!(pusmogo_single_line(pr).is_null());
            assert!(pusmogo_style_apply(pr, 3, 0, hu.as_ptr()).is_null());
            assert_eq!(pusmogo_style_category(pr, pr, pr), 0);
            assert!(pusmogo_learned_correction(pr, pr, pr).is_null());
            assert_eq!(pusmogo_model_wanted(pr, 0, 0), 0);
            assert_eq!(pusmogo_model_pick(pr, 0, 0, 1, 1, 1), -1);

            // A panic with a live engine: the handle stays usable afterwards.
            let h = pusmogo_engine_new(ptr::null());
            assert!(!h.is_null());
            let samples = vec![0.0f32; 1600];
            assert!(pusmogo_transcribe(h, samples.as_ptr(), samples.len(), pr).is_null());
            assert_eq!(pusmogo_set_context(h, pr), -1);
            pusmogo_dictionary_add(h, pr, pr);
            let out = pusmogo_transcribe(h, samples.as_ptr(), samples.len(), hu.as_ptr());
            assert!(!out.is_null());
            pusmogo_string_free(out);
            pusmogo_engine_free(h);

            // Profile entry points.
            let path = std::env::temp_dir().join(format!("pusmogo-ffi-panic-{}.db", std::process::id()));
            let _ = std::fs::remove_file(&path);
            assert!(pusmogo_profile_open(pr).is_null());
            assert_eq!(pusmogo_profile_is_newer(pr), 0);
            let p = pusmogo_profile_open(CString::new(path.to_str().unwrap()).unwrap().as_ptr());
            assert!(!p.is_null());
            assert_eq!(pusmogo_profile_add_history(p, hu.as_ptr(), pr, 300, ptr::null()), -1);
            assert!(pusmogo_profile_search_history(p, pr, 10).is_null());
            assert_eq!(pusmogo_profile_set_dictionary(p, pr, pr), -1);
            assert_eq!(pusmogo_profile_set_snippet(p, pr, pr), -1);
            assert!(pusmogo_profile_snippet_for(p, pr).is_null());
            assert_eq!(pusmogo_profile_set_setting(p, pr, pr), -1);
            assert!(pusmogo_profile_add_history(p, hu.as_ptr(), hu.as_ptr(), 300, ptr::null()) > 0);
            pusmogo_profile_free(p);
            let _ = std::fs::remove_file(&path);

            // Other hosts' entry points.
            assert!(pusmogo_mcp_config_path(pr, pr).is_null());
            assert_eq!(pusmogo_mcp_install(pr, pr, pr), -1);
            assert!(pusmogo_testset_status(pr).is_null());
        }
    }

    #[test]
    fn integration_api_through_c_abi() {
        let c = |s: &str| CString::new(s).unwrap();
        unsafe {
            let out = pusmogo_clean_text(c("szia vessző hogy vagy kérdőjel").as_ptr());
            assert!(!out.is_null());
            let cleaned = CStr::from_ptr(out).to_str().unwrap().to_string();
            pusmogo_string_free(out);
            assert_eq!(cleaned, "Szia, hogy vagy?");
            assert!(pusmogo_clean_text(ptr::null()).is_null());

            let h = pusmogo_engine_new(ptr::null());
            assert!(!h.is_null());
            pusmogo_dictionary_add(h, c("klózisz").as_ptr(), c("Clausis").as_ptr());
            assert_eq!(finish_text(&*h, "Holnap a klózisz csapattal.", Language::Hungarian), "Holnap a Clausis csapattal.");
            pusmogo_dictionary_add(h, ptr::null(), c("x").as_ptr());
            pusmogo_dictionary_add(ptr::null_mut(), c("a").as_ptr(), c("b").as_ptr());
            pusmogo_engine_free(h);

            let path = std::env::temp_dir().join(format!("pusmogo-ffi-stats-{}.db", std::process::id()));
            let _ = std::fs::remove_file(&path);
            let p = pusmogo_profile_open(c(path.to_str().unwrap()).as_ptr());
            assert!(!p.is_null());
            let (hu, app) = (c("hu"), c("com.apple.TextEdit"));
            for (text, ms) in [("egy kettő három", 300), ("négy öt", 500), ("hat", 900)] {
                assert!(pusmogo_profile_add_history(p, hu.as_ptr(), c(text).as_ptr(), ms, app.as_ptr()) > 0);
            }
            let json = pusmogo_profile_stats(p);
            assert!(!json.is_null());
            assert_eq!(CStr::from_ptr(json).to_str().unwrap(), r#"{"dictations":3,"words":6,"median_latency_ms":500}"#);
            pusmogo_string_free(json);
            assert!(pusmogo_profile_stats(ptr::null_mut()).is_null());
            pusmogo_profile_free(p);
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn debug_dir_keeps_dictations_from_both_paths() {
        let dir = std::env::temp_dir().join(format!("pusmogo-ffi-debug-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c_dir = CString::new(dir.to_str().unwrap()).unwrap();
        let hu = CString::new("hu").unwrap();
        let audio = vec![0.0f32; 16_000];
        let count = || std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
        unsafe {
            let h = pusmogo_engine_new(ptr::null());
            pusmogo_string_free(pusmogo_transcribe(h, audio.as_ptr(), audio.len(), hu.as_ptr()));
            assert_eq!(count(), 0, "off by default");
            assert_eq!(pusmogo_set_debug_dir(h, c_dir.as_ptr()), 0);
            pusmogo_string_free(pusmogo_transcribe(h, audio.as_ptr(), audio.len(), hu.as_ptr()));
            assert_eq!(count(), 2);
            std::thread::sleep(std::time::Duration::from_millis(2));
            let s = pusmogo_session_new(hu.as_ptr());
            pusmogo_session_push(s, audio.as_ptr(), audio.len());
            pusmogo_string_free(pusmogo_session_finish(h, s));
            pusmogo_session_free(s);
            assert_eq!(count(), 4);
            let json = std::fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| p.extension().is_some_and(|x| x == "json"))
                .unwrap();
            let json = std::fs::read_to_string(json).unwrap();
            assert!(json.contains("\"lang\": \"hu\"") && json.contains("\"audio_secs\": 1.00"), "{json}");
            assert_eq!(pusmogo_set_debug_dir(h, ptr::null()), 0);
            pusmogo_string_free(pusmogo_transcribe(h, audio.as_ptr(), audio.len(), hu.as_ptr()));
            assert_eq!(count(), 4, "off again");
            assert_eq!(pusmogo_set_debug_dir(ptr::null_mut(), c_dir.as_ptr()), -1);
            pusmogo_engine_free(h);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn roundtrip_through_c_abi() {
        unsafe {
            let h = pusmogo_engine_new(ptr::null());
            assert!(!h.is_null());
            let samples = vec![0.1f32; 8_000];
            let lang = CString::new("hu").unwrap();
            let out = pusmogo_transcribe(h, samples.as_ptr(), samples.len(), lang.as_ptr());
            let text = CStr::from_ptr(out).to_str().unwrap().to_string();
            pusmogo_string_free(out);
            pusmogo_engine_free(h);
            assert!(text.contains("0.5 s"), "{text}");
        }
    }

    #[test]
    fn session_through_c_abi() {
        unsafe {
            let h = pusmogo_engine_new(ptr::null());
            let lang = CString::new("hu").unwrap();
            let s = pusmogo_session_new(lang.as_ptr());
            pusmogo_session_set_previews(s, 1);
            // One second of syllable-like bursts: previews only run on speech.
            let samples: Vec<f32> =
                (0..16_000).map(|i| if i % 4000 < 2400 { 0.2 * (i as f32 * 0.09).sin() } else { 0.001 }).collect();
            // Under the minimum chunk: no chunk, but a caption preview at 4 s.
            let steps: Vec<i32> = (0..5)
                .map(|_| {
                    pusmogo_session_push(s, samples.as_ptr(), samples.len());
                    pusmogo_session_step(h, s)
                })
                .collect();
            assert_eq!(steps, [0, 0, 0, 1, 0]);
            assert_eq!(pusmogo_session_step(ptr::null_mut(), s), -1);
            let live = pusmogo_session_text(h, s);
            assert!(CStr::from_ptr(live).to_str().unwrap().contains("4.0 s"), "preview of the first 4 s");
            pusmogo_string_free(live);
            assert!(pusmogo_session_text(ptr::null(), s).is_null());
            // After an interrupt no preview runs, and the final pass covers everything.
            pusmogo_session_interrupt();
            pusmogo_session_push(s, samples.as_ptr(), samples.len());
            assert_eq!(pusmogo_session_step(h, s), 0);
            let out = pusmogo_session_finish(h, s);
            let text = CStr::from_ptr(out).to_str().unwrap().to_string();
            pusmogo_string_free(out);
            pusmogo_session_free(s);
            pusmogo_engine_free(h);
            assert!(text.contains("6.0 s"), "{text}");
        }
    }

    #[test]
    fn context_through_c_abi() {
        unsafe {
            let text = CString::new("Holnap írok Kovács Bencének.").unwrap();
            assert_eq!(pusmogo_set_context(ptr::null_mut(), text.as_ptr()), -1);
            let h = pusmogo_engine_new(ptr::null());
            assert_eq!(pusmogo_set_context(h, text.as_ptr()), 1);
            // 30 ms frames alternating loud and silent: half the audio is voiced.
            let speech = |secs: f32| -> Vec<f32> {
                (0..(secs * 16_000.0) as usize).map(|i| if (i / 480) % 2 == 0 { 0.3 * (i as f32 * 0.05).sin() } else { 0.0 }).collect()
            };
            let (long, short) = (speech(6.0), speech(1.0));
            assert_eq!(dict_prompt(&*h, Language::Hungarian, &long), format!("{} Kovács Bencének.", crate::cleanup::HU_PROMPT_LABEL));
            assert_eq!(dict_prompt(&*h, Language::English, &long), "Common words: Kovács Bencének.");
            assert_eq!(dict_prompt(&*h, Language::Hungarian, &short), "");
            assert_eq!(pusmogo_set_context(h, ptr::null()), 0);
            let code = CString::new("main.rs — fix user_id").unwrap();
            assert_eq!(pusmogo_set_code_context(ptr::null_mut(), code.as_ptr()), -1);
            assert_eq!(pusmogo_set_code_context(h, code.as_ptr()), 2);
            assert_eq!(pusmogo_set_context(h, code.as_ptr()), 0);
            assert_eq!(dict_prompt(&*h, Language::Hungarian, &long), "");
            pusmogo_engine_free(h);
        }
    }

    #[test]
    fn warm_up_through_c_abi() {
        unsafe {
            assert_eq!(pusmogo_engine_warm_up(ptr::null_mut()), -1);
            let h = pusmogo_engine_new(ptr::null());
            assert_eq!(pusmogo_engine_warm_up(h), 0);
            pusmogo_engine_free(h);
        }
    }

    #[test]
    fn insights_through_c_abi() {
        unsafe {
            let path = std::env::temp_dir().join(format!("pusmogo-ffi-ins-{}.db", std::process::id()));
            let _ = std::fs::remove_file(&path);
            let cpath = CString::new(path.to_str().unwrap()).unwrap();
            let p = pusmogo_profile_open(cpath.as_ptr());
            assert!(!p.is_null());
            let (hu, app) = (CString::new("hu").unwrap(), CString::new("com.apple.mail").unwrap());
            let words = ["szia"; 40].join(" ");
            let text = CString::new(words).unwrap();
            let id = pusmogo_profile_add_history(p, hu.as_ptr(), text.as_ptr(), 900, app.as_ptr());
            assert!(id > 0);
            // 40 words in 30 s of audio: 80 words per minute.
            assert_eq!(pusmogo_profile_set_audio_ms(p, id, 30_000), 0);
            let (w, r) = (CString::new("klózisz").unwrap(), CString::new("Clausis").unwrap());
            assert_eq!(pusmogo_profile_add_correction(p, w.as_ptr(), r.as_ptr()), 0);
            let empty = CString::new(" ").unwrap();
            assert_eq!(pusmogo_profile_add_correction(p, w.as_ptr(), empty.as_ptr()), -1);
            assert_eq!(pusmogo_profile_note_correction(p, w.as_ptr(), r.as_ptr()), 0);
            assert_eq!(pusmogo_profile_note_correction(p, w.as_ptr(), r.as_ptr()), 1);
            assert_eq!(pusmogo_profile_note_correction(p, w.as_ptr(), r.as_ptr()), 0);
            assert_eq!(pusmogo_profile_note_correction(p, w.as_ptr(), empty.as_ptr()), -1);
            assert_eq!(pusmogo_profile_note_correction(ptr::null_mut(), w.as_ptr(), r.as_ptr()), -1);
            assert_eq!(pusmogo_profile_add_correction(p, ptr::null(), r.as_ptr()), -1);
            let json = pusmogo_profile_insights(p, 3600);
            assert!(!json.is_null());
            let s = CStr::from_ptr(json).to_str().unwrap().to_string();
            pusmogo_string_free(json);
            assert!(s.contains(r#""dictations":1"#), "{s}");
            assert!(s.contains(r#""words":40"#), "{s}");
            assert!(s.contains(r#""words_per_minute":80"#), "{s}");
            assert!(s.contains(r#""top_correction":{"text":"Clausis","count":1}"#), "{s}");
            assert!(s.contains(r#""top_app":{"text":"com.apple.mail","count":1}"#), "{s}");
            assert!(pusmogo_profile_insights(ptr::null_mut(), 0).is_null());
            assert_eq!(pusmogo_profile_set_audio_ms(ptr::null_mut(), id, 1), -1);
            pusmogo_profile_free(p);
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn profile_through_c_abi() {
        unsafe {
            let path = std::env::temp_dir().join(format!("pusmogo-ffi-{}.db", std::process::id()));
            let _ = std::fs::remove_file(&path);
            let cpath = CString::new(path.to_str().unwrap()).unwrap();
            let p = pusmogo_profile_open(cpath.as_ptr());
            assert!(!p.is_null());
            let hu = CString::new("hu").unwrap();
            let text = CString::new("A klózisz csapat \"ír\"").unwrap();
            assert!(pusmogo_profile_add_history(p, hu.as_ptr(), text.as_ptr(), 800, ptr::null()) > 0);
            let q = CString::new("csapat").unwrap();
            let json = pusmogo_profile_search_history(p, q.as_ptr(), 10);
            let s = CStr::from_ptr(json).to_str().unwrap().to_string();
            pusmogo_string_free(json);
            assert!(s.contains(r#""text":"A klózisz csapat \"ír\"""#), "{s}");

            let (from, to) = (CString::new("klózisz").unwrap(), CString::new("Clausis").unwrap());
            assert_eq!(pusmogo_profile_set_dictionary(p, from.as_ptr(), to.as_ptr()), 0);
            let h = pusmogo_engine_new(ptr::null());
            assert_eq!(pusmogo_profile_apply_dictionary(p, h), 0);
            assert_eq!((*h).dict.prompt(), format!("{} Clausis.", crate::cleanup::HU_PROMPT_LABEL));
            // The stub engine has no tokenizer: the fit is unknown.
            assert_eq!(pusmogo_dictionary_prompt_left_out(h), -1);
            assert_eq!(pusmogo_dictionary_prompt_left_out(ptr::null()), -1);

            let key = CString::new("history_days").unwrap();
            assert!(pusmogo_profile_setting(p, key.as_ptr()).is_null());
            let v = CString::new("30").unwrap();
            assert_eq!(pusmogo_profile_set_setting(p, key.as_ptr(), v.as_ptr()), 0);
            let got = pusmogo_profile_setting(p, key.as_ptr());
            assert_eq!(CStr::from_ptr(got).to_str().unwrap(), "30");
            pusmogo_string_free(got);
            assert_eq!(pusmogo_profile_prune_history(p, 30), 0);
            assert_eq!(pusmogo_profile_set_dictionary(ptr::null_mut(), from.as_ptr(), to.as_ptr()), -1);

            let (trig, body) = (CString::new("Címem").unwrap(), CString::new("Példa utca 1.\n1051 Budapest").unwrap());
            let id = pusmogo_profile_set_snippet(p, trig.as_ptr(), body.as_ptr());
            assert!(id > 0);
            let said = CString::new("cimem!").unwrap();
            let got = pusmogo_profile_snippet_for(p, said.as_ptr());
            assert_eq!(CStr::from_ptr(got).to_str().unwrap(), "Példa utca 1.\n1051 Budapest");
            pusmogo_string_free(got);
            assert!(pusmogo_profile_snippet_for(p, text.as_ptr()).is_null());
            let list = pusmogo_profile_snippets(p);
            assert!(CStr::from_ptr(list).to_str().unwrap().contains(r#""trigger":"Címem","text":"Példa utca 1.\n1051 Budapest""#));
            pusmogo_string_free(list);
            let new_trig = CString::new("otthon").unwrap();
            assert_eq!(pusmogo_profile_update_snippet(p, id, new_trig.as_ptr(), body.as_ptr()), 0);
            assert!(pusmogo_profile_snippet_for(p, said.as_ptr()).is_null());
            assert_eq!(pusmogo_profile_delete_snippet(p, id), 0);
            assert_eq!(pusmogo_profile_set_snippet(ptr::null_mut(), trig.as_ptr(), body.as_ptr()), -1);

            pusmogo_engine_free(h);
            pusmogo_profile_free(p);
            let _ = std::fs::remove_file(&path);
        }
    }
}
