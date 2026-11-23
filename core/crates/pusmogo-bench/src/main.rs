//! pusmogo-bench: score a speech engine on manifests of WAV clips + reference text.
//!
//! Usage:
//!   pusmogo-bench [--model PATH] [--json OUT.json] [--md OUT.md] [--no-warmup] [--stream [--preview] [--abort-probe] [--release-after S] [--step-every S]] [--prompt TEXT | --prompt-file F | --dict-file F] [--context-file F] [--hyp-file F] MANIFEST.tsv...
//!
//! `--stream` feeds each clip to a streaming `Session` in 100 ms pieces and steps it
//! once per second of audio, as the app does while recording. Background chunk
//! time is not user-visible (it happens while the user speaks), so the reported
//! latency is only `finish()` plus cleanup: the wait after the key is released.
//! With `--preview` it makes caption previews when no chunk is ready, as the
//! app does with `PUSMOGO_CAPTION_PREVIEW=1`. It reports when the first
//! caption appeared, with previews and from chunks alone.
//! Steps are simulated in real time: a step is skipped while the previous
//! one would still be running, and the time a pass would still run after the
//! end of the clip is reported as the release wait (added to the latency the
//! user feels, on top of `finish()`). `--abort-probe` also starts a
//! preview of the last 6 s at release, stops it after 100 ms and reports how
//! long the engine took to give up (the most a release can wait behind one),
//! then does the same for a 20 s chunk pass (full window, final decoding).
//! `--release-after S` appends S seconds of quiet room tone to each streamed
//! clip before the release, since a real release lags the last word.
//! `--step-every S` steps every S seconds of audio instead of every second
//! (a multiple of 0.1 s), to try a slower step for a slower model.
//!
//! Manifest: one clip per line, `id<TAB>lang<TAB>reference` (further columns, such as a
//! speaker, are ignored); `<id>.wav` sits next to
//! the manifest (or the id is a path ending in .wav, relative to the manifest).
//! Model: --model, else $PUSMOGO_MODEL, else the app's default turbo q5 path.
//! Build with `--features whisper` for the real engine; without it the stub runs.
//!
//! `--hyp-file F` scores another engine's output instead of running one: F holds
//! `id<TAB>seconds<TAB>raw text` per clip (seconds = that engine's time for the
//! clip). The raw text goes through the same `respell` + `clean()` as below.
//!
//! Every clip goes through the same path as the app: engine → `clean()`. Scoring
//! normalises both sides (see `normalize`). The engine runs its warm-up (as the app
//! does at launch) before the timed clips, and that time is reported separately.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pusmogo_bench::metrics::{char_count, percentile, word_count, word_count_joined, Count};
use pusmogo_bench::normalize::{fold_accents, normalize};
use pusmogo_core::{audio, clean, engine, session, Dictionary, Language, Session, SAMPLE_RATE};
use serde_json::{json, Value};

struct Clip {
    set: String,
    id: String,
    lang: Language,
    lang_code: String,
    wav: PathBuf,
    reference: String,
}

struct Row {
    clip: Clip,
    audio_s: f64,
    latency_s: f64,
    output: String,
    wer: Count,
    wer_joined: Count,
    cer: Count,
    cer_folded: Count,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("pusmogo-bench: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut model = std::env::var("PUSMOGO_MODEL").ok();
    let mut json_out: Option<String> = None;
    let mut md_out: Option<String> = None;
    let mut warmup = true;
    let mut stream = false;
    let mut abort_probe = false;
    let mut previews = false;
    let mut release_after = 0f64;
    let mut release_waits: Vec<f64> = Vec::new();
    let mut prompt = String::new();
    let mut dict = Dictionary::new();
    let mut context: Vec<String> = Vec::new();
    let mut manifests = Vec::new();
    let mut hyp_file: Option<String> = None;
    // Steps every this many 100 ms pieces of streamed audio (10 = once a second).
    let mut step_ticks = 10usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--model" => model = Some(args.next().ok_or("--model needs a path")?),
            "--json" => json_out = Some(args.next().ok_or("--json needs a path")?),
            "--md" => md_out = Some(args.next().ok_or("--md needs a path")?),
            "--no-warmup" => warmup = false,
            "--stream" => stream = true,
            "--abort-probe" => abort_probe = true,
            "--preview" => previews = true,
            "--step-every" => {
                let v = args.next().ok_or("--step-every needs seconds")?;
                let s: f64 = v.parse().map_err(|_| format!("--step-every: not a number: {v}"))?;
                step_ticks = ((s * 10.0).round() as usize).max(1);
            }
            "--release-after" => {
                let v = args.next().ok_or("--release-after needs seconds")?;
                release_after = v.parse().map_err(|_| format!("--release-after: not a number: {v}"))?;
            }
            "--prompt" => prompt = args.next().ok_or("--prompt needs text")?,
            "--prompt-file" => {
                let p = args.next().ok_or("--prompt-file needs a path")?;
                prompt = std::fs::read_to_string(&p).map_err(|e| format!("cannot read {p}: {e}"))?.trim().to_string();
            }
            "--dict-file" => {
                // One entry per line: "Spelling" or "spoken<TAB>Spelling", as in
                // the app's dictionary. Its prompt is fitted to Whisper's budget.
                let p = args.next().ok_or("--dict-file needs a path")?;
                let text = std::fs::read_to_string(&p).map_err(|e| format!("cannot read {p}: {e}"))?;
                for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    let (from, to) = line.split_once('\t').unwrap_or((line, line));
                    dict.add(from.trim(), to.trim());
                }
            }
            "--context-file" => {
                // Text around the cursor, as the app reads it from the focused
                // field: its name-like words are added to the prompt.
                let p = args.next().ok_or("--context-file needs a path")?;
                let text = std::fs::read_to_string(&p).map_err(|e| format!("cannot read {p}: {e}"))?;
                context = pusmogo_core::context::hint_words(&text, pusmogo_core::context::MAX_HINT_WORDS);
                eprintln!("context hints: {}", context.join(", "));
            }
            "--hyp-file" => hyp_file = Some(args.next().ok_or("--hyp-file needs a path")?),
            "-h" | "--help" => {
                println!("usage: pusmogo-bench [--model PATH] [--json OUT] [--md OUT] [--no-warmup] [--stream [--preview] [--abort-probe] [--release-after S] [--step-every S]] [--prompt TEXT | --prompt-file F | --dict-file F] [--context-file F] [--hyp-file F] MANIFEST.tsv...");
                return Ok(());
            }
            _ => manifests.push(a),
        }
    }
    if manifests.is_empty() {
        return Err("no manifest given (try --help)".into());
    }
    let model = model.or_else(default_model);

    let mut clips = Vec::new();
    for m in &manifests {
        clips.extend(read_manifest(Path::new(m))?);
    }
    if clips.is_empty() {
        return Err("manifests hold no clips".into());
    }

    if let Some(p) = hyp_file {
        let hyps = read_hyps(Path::new(&p))?;
        let mut rows = Vec::new();
        for clip in clips {
            let (latency_s, raw) = hyps.get(&clip.id).ok_or(format!("{p}: no output for clip {}", clip.id))?.clone();
            let audio_s = audio::read_wav_16k_mono(&clip.wav.to_string_lossy())?.len() as f64 / SAMPLE_RATE as f64;
            let output = clean(&pusmogo_core::context::respell(&raw, &context), &dict);
            rows.push(score(clip, audio_s, latency_s, output));
        }
        return report(&rows, "external", Some(&p), 0.0, None, md_out, json_out);
    }

    let t_load = Instant::now();
    let mut eng = engine::load(model.as_deref()).map_err(|e| e.to_string())?;
    let load_s = t_load.elapsed().as_secs_f64();
    eprintln!("engine {} loaded in {load_s:.2} s{}", eng.name(), if stream { ", streaming" } else { "" });
    // One prompt per dictation language, as the app builds it (the label is
    // in the clip's language); `--prompt` text is used as given for every clip.
    let langs = [Language::Hungarian, Language::English, Language::Auto];
    let prompts: Vec<String> = langs
        .iter()
        .map(|&l| {
            if prompt.is_empty() {
                dict.prompt_fitting_with(&context, l, engine::MAX_PROMPT_TOKENS, |s| eng.count_tokens(s))
            } else {
                prompt.clone()
            }
        })
        .collect();
    for (l, p) in langs.iter().zip(&prompts) {
        if !p.is_empty() && clips.iter().any(|c| c.lang == *l) {
            let tokens = eng.count_tokens(p).map_or("?".to_string(), |n| n.to_string());
            eprintln!("prompt ({l:?}): {} chars, {tokens} tokens (Whisper keeps the last 224)", p.chars().count());
        }
    }

    let mut cold_s = None;
    if warmup {
        // Same warm-up as the app, so clip latencies match what users feel.
        let t = Instant::now();
        eng.warm_up().map_err(|e| e.to_string())?;
        cold_s = Some(t.elapsed().as_secs_f64());
    }

    let given_prompt = !prompt.is_empty();
    let mut rows = Vec::new();
    for clip in clips {
        let mut samples = audio::read_wav_16k_mono(&clip.wav.to_string_lossy())?;
        if stream && release_after > 0.0 {
            samples = pusmogo_bench::room::with_room_tone(&samples, release_after, SAMPLE_RATE);
        }
        let audio_s = samples.len() as f64 / SAMPLE_RATE as f64;
        let mut prompt = &prompts[langs.iter().position(|l| *l == clip.lang).unwrap_or(0)];
        // As in the app, context hints stay out of the prompt of a short
        // dictation (`context::prompt_hints`).
        let short_prompt;
        if !given_prompt && !context.is_empty() && pusmogo_core::context::prompt_hints(&context, &samples).is_empty() {
            short_prompt = dict.prompt_fitting_with(&[], clip.lang, engine::MAX_PROMPT_TOKENS, |s| eng.count_tokens(s));
            prompt = &short_prompt;
        }
        let (output, latency_s) = if stream {
            let mut session = Session::new(clip.lang);
            let stop = AtomicBool::new(false);
            let (mut slowest_step, mut slowest_preview) = (0f64, 0f64);
            // Seconds into the dictation when the caption first showed text,
            // with previews and (as before them) from chunks alone.
            let (mut first_caption, mut first_chunk) = (None, None);
            // Audio time (s) until which the engine is busy with a pass.
            let mut busy_until = 0f64;
            let piece = SAMPLE_RATE as usize / 10;
            for (i, chunk) in samples.chunks(piece).enumerate() {
                session.push(chunk);
                let now = session.total_secs() as f64;
                if i % step_ticks == step_ticks - 1 && now >= busy_until {
                    let t = Instant::now();
                    let mut ran = session.step(eng.as_mut(), prompt).map_err(|e| e.to_string())?;
                    if ran {
                        slowest_step = slowest_step.max(t.elapsed().as_secs_f64());
                    } else if previews && session.preview(eng.as_mut(), prompt, &stop).map_err(|e| e.to_string())? {
                        slowest_preview = slowest_preview.max(t.elapsed().as_secs_f64());
                        ran = true;
                    }
                    if ran {
                        busy_until = now + t.elapsed().as_secs_f64();
                    }
                    let at = busy_until.max(now) as f32;
                    if first_caption.is_none() && !session.caption().is_empty() {
                        first_caption = Some(at);
                    }
                    if first_chunk.is_none() && !session.text().is_empty() {
                        first_chunk = Some(at);
                    }
                }
            }
            if abort_probe {
                let tail = &samples[samples.len().saturating_sub(6 * SAMPLE_RATE as usize)..];
                let probe = AtomicBool::new(false);
                let given_up = std::thread::scope(|scope| {
                    scope.spawn(|| {
                        std::thread::sleep(Duration::from_millis(100));
                        probe.store(true, Ordering::Relaxed);
                    });
                    let t = Instant::now();
                    let r = eng.transcribe_until(tail, clip.lang, prompt, &probe);
                    (t.elapsed().as_secs_f64() - 0.1, r.map(|o| o.is_none()))
                });
                eprintln!("  abort probe: gave up {:.0} ms after the stop ({:?})", given_up.0 * 1000.0, given_up.1);
                // The same for a full chunk pass (full window, final decoding),
                // which a release abandons when the final pass would redo it.
                let tail = &samples[samples.len().saturating_sub(20 * SAMPLE_RATE as usize)..];
                let t = Instant::now();
                eng.transcribe(tail, clip.lang, prompt).map_err(|e| e.to_string())?;
                let whole = t.elapsed().as_secs_f64();
                let probe = AtomicBool::new(false);
                let given_up = std::thread::scope(|scope| {
                    scope.spawn(|| {
                        std::thread::sleep(Duration::from_millis(100));
                        probe.store(true, Ordering::Relaxed);
                    });
                    let t = Instant::now();
                    let r = eng.transcribe_abortable(tail, clip.lang, prompt, &probe);
                    (t.elapsed().as_secs_f64() - 0.1, r.map(|o| o.is_none()))
                });
                eprintln!(
                    "  chunk abort probe: 20 s pass {:.0} ms whole, gave up {:.0} ms after the stop ({:?})",
                    whole * 1000.0,
                    given_up.0 * 1000.0,
                    given_up.1
                );
            }
            let release_wait = (busy_until - session.total_secs() as f64).max(0.0);
            release_waits.push(release_wait);
            let (chunks, tail_s) = (session.chunks_done(), session.pending_secs());
            let t = Instant::now();
            let raw = session.finish(eng.as_mut(), prompt).map_err(|e| e.to_string())?;
            let output = clean(&pusmogo_core::context::respell(&raw, &context), &dict);
            let latency_s = t.elapsed().as_secs_f64();
            let secs = |s: Option<f32>| s.map_or("none".to_string(), |s| format!("{s:.1} s"));
            eprintln!(
                "  stream: {chunks} chunks before release, tail {tail_s:.1} s, release wait {release_wait:.2} s, slowest step {slowest_step:.2} s, slowest preview {slowest_preview:.2} s, first caption {} (chunks alone {})",
                secs(first_caption),
                secs(first_chunk)
            );
            (output, latency_s)
        } else {
            let t = Instant::now();
            let raw = session::transcribe_all(eng.as_mut(), &samples, clip.lang, prompt).map_err(|e| e.to_string())?;
            let output = clean(&pusmogo_core::context::respell(&raw, &context), &dict);
            (output, t.elapsed().as_secs_f64())
        };

        rows.push(score(clip, audio_s, latency_s, output));
    }
    if !release_waits.is_empty() {
        let waited = release_waits.iter().filter(|w| **w > 0.0).count();
        let mean = release_waits.iter().sum::<f64>() / release_waits.len() as f64;
        let max = release_waits.iter().copied().fold(0.0, f64::max);
        eprintln!(
            "release wait behind a running pass: {waited} of {} clips, mean {mean:.2} s, max {max:.2} s",
            release_waits.len()
        );
    }

    report(&rows, eng.name(), model.as_deref(), load_s, cold_s, md_out, json_out)
}

/// Scores one clip's cleaned output against its reference and logs the row.
fn score(clip: Clip, audio_s: f64, latency_s: f64, output: String) -> Row {
    let r = normalize(&clip.reference, clip.lang);
    let h = normalize(&output, clip.lang);
    let row = Row {
        wer: word_count(&r, &h),
        wer_joined: word_count_joined(&r, &h),
        cer: char_count(&r, &h),
        cer_folded: char_count(&fold_accents(&r), &fold_accents(&h)),
        audio_s,
        latency_s,
        output,
        clip,
    };
    eprintln!("{} {:.2}s WER {:.0}% | {}", row.clip.id, row.latency_s, row.wer.rate() * 100.0, row.output);
    row
}

fn report(
    rows: &[Row],
    engine: &str,
    model: Option<&str>,
    load_s: f64,
    cold_s: Option<f64>,
    md_out: Option<String>,
    json_out: Option<String>,
) -> Result<(), String> {
    let md = markdown(rows, engine, model, load_s, cold_s);
    print!("{md}");
    if let Some(p) = md_out {
        std::fs::write(&p, &md).map_err(|e| format!("cannot write {p}: {e}"))?;
    }
    if let Some(p) = json_out {
        let j = to_json(rows, engine, model, load_s, cold_s);
        std::fs::write(&p, serde_json::to_string_pretty(&j).unwrap() + "\n").map_err(|e| format!("cannot write {p}: {e}"))?;
    }
    Ok(())
}

/// Reads `id<TAB>seconds<TAB>raw text` lines (another engine's output per clip).
fn read_hyps(path: &Path) -> Result<std::collections::HashMap<String, (f64, String)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut hyps = std::collections::HashMap::new();
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let mut parts = line.splitn(3, '\t');
        let (id, secs, raw) = (parts.next().unwrap_or(""), parts.next(), parts.next().unwrap_or(""));
        let secs = secs
            .and_then(|s| s.trim().parse().ok())
            .ok_or(format!("{}:{}: expected id<TAB>seconds<TAB>text", path.display(), n + 1))?;
        hyps.insert(id.to_string(), (secs, raw.to_string()));
    }
    Ok(hyps)
}

fn default_model() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    Some(format!("{home}/Library/Application Support/Pusmogo/models/ggml-large-v3-turbo-q5_0.bin"))
}

fn read_manifest(path: &Path) -> Result<Vec<Clip>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let base = path.parent().unwrap_or(Path::new("."));
    let set = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut clips = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(4, '\t');
        let (Some(id), Some(lang), Some(reference)) = (parts.next(), parts.next(), parts.next()) else {
            return Err(format!("{}:{}: want id<TAB>lang<TAB>reference", path.display(), n + 1));
        };
        let wav = if id.ends_with(".wav") { base.join(id) } else { base.join(format!("{id}.wav")) };
        if !wav.exists() {
            return Err(format!("{}:{}: missing {}", path.display(), n + 1, wav.display()));
        }
        clips.push(Clip {
            set: set.clone(),
            id: id.trim_end_matches(".wav").to_string(),
            lang: Language::from_code(lang),
            lang_code: lang.to_string(),
            wav,
            reference: reference.to_string(),
        });
    }
    Ok(clips)
}

struct Summary {
    name: String,
    clips: usize,
    audio_s: f64,
    wer: Count,
    wer_joined: Count,
    cer: Count,
    cer_folded: Count,
    rtf: f64,
    p50: f64,
    p95: f64,
}

/// One summary per (set, lang) group plus "all".
fn summaries(rows: &[Row]) -> Vec<Summary> {
    let mut groups: Vec<(String, Vec<&Row>)> = Vec::new();
    for r in rows {
        let key = format!("{}/{}", r.clip.set, r.clip.lang_code);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(r),
            None => groups.push((key, vec![r])),
        }
    }
    if groups.len() > 1 {
        groups.push(("all".into(), rows.iter().collect()));
    }
    groups
        .into_iter()
        .map(|(name, g)| {
            let mut s = Summary {
                name,
                clips: g.len(),
                audio_s: 0.0,
                wer: Count::default(),
                wer_joined: Count::default(),
                cer: Count::default(),
                cer_folded: Count::default(),
                rtf: 0.0,
                p50: 0.0,
                p95: 0.0,
            };
            let lat: Vec<f64> = g.iter().map(|r| r.latency_s).collect();
            for r in &g {
                s.audio_s += r.audio_s;
                s.wer.add(r.wer);
                s.wer_joined.add(r.wer_joined);
                s.cer.add(r.cer);
                s.cer_folded.add(r.cer_folded);
            }
            s.rtf = if s.audio_s > 0.0 { lat.iter().sum::<f64>() / s.audio_s } else { 0.0 };
            s.p50 = percentile(&lat, 50.0);
            s.p95 = percentile(&lat, 95.0);
            s
        })
        .collect()
}

fn pct(c: Count) -> String {
    format!("{:.1}", c.rate() * 100.0)
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

fn markdown(rows: &[Row], engine: &str, model: Option<&str>, load_s: f64, cold_s: Option<f64>) -> String {
    let mut out = String::new();
    let model_name = model.and_then(|m| Path::new(m).file_name()).map(|f| f.to_string_lossy().into_owned());
    out.push_str(&format!(
        "Engine `{engine}`{}, load {load_s:.2} s{}.\n\n",
        model_name.map(|m| format!(" (`{m}`)")).unwrap_or_default(),
        cold_s.map(|c| format!(", warm-up {c:.2} s (excluded below)")).unwrap_or_default()
    ));
    out.push_str("| set | clips | audio s | WER % | WER joined % | CER % | CER no-accent % | RTF | p50 s | p95 s |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|---|\n");
    for s in summaries(rows) {
        out.push_str(&format!(
            "| {} | {} | {:.1} | {} | {} | {} | {} | {:.3} | {:.2} | {:.2} |\n",
            s.name, s.clips, s.audio_s, pct(s.wer), pct(s.wer_joined), pct(s.cer), pct(s.cer_folded), s.rtf, s.p50, s.p95
        ));
    }
    out.push_str("\n| id | lang | audio s | latency s | WER % | reference | output |\n|---|---|---|---|---|---|---|\n");
    for r in rows {
        out.push_str(&format!(
            "| {} | {} | {:.1} | {:.2} | {} | {} | {} |\n",
            r.clip.id, r.clip.lang_code, r.audio_s, r.latency_s, pct(r.wer), cell(&r.clip.reference), cell(&r.output)
        ));
    }
    out
}

fn count_json(c: Count) -> Value {
    json!({ "errors": c.errors, "len": c.len, "rate": c.rate() })
}

fn to_json(rows: &[Row], engine: &str, model: Option<&str>, load_s: f64, cold_s: Option<f64>) -> Value {
    json!({
        "engine": engine,
        "model": model,
        "load_s": load_s,
        "cold_first_call_s": cold_s,
        "summary": summaries(rows).iter().map(|s| json!({
            "set": s.name, "clips": s.clips, "audio_s": s.audio_s,
            "wer": count_json(s.wer), "wer_joined": count_json(s.wer_joined), "cer": count_json(s.cer), "cer_no_accent": count_json(s.cer_folded),
            "rtf": s.rtf, "latency_p50_s": s.p50, "latency_p95_s": s.p95,
        })).collect::<Vec<_>>(),
        "clips": rows.iter().map(|r| json!({
            "set": r.clip.set, "id": r.clip.id, "lang": r.clip.lang_code,
            "audio_s": r.audio_s, "latency_s": r.latency_s,
            "reference": r.clip.reference, "output": r.output,
            "wer": count_json(r.wer), "wer_joined": count_json(r.wer_joined), "cer": count_json(r.cer), "cer_no_accent": count_json(r.cer_folded),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    #[test]
    fn columns_after_the_reference_are_not_part_of_it() {
        let dir = std::env::temp_dir().join(format!("pusmogo-bench-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.wav"), b"").unwrap();
        std::fs::write(dir.join("b.wav"), b"").unwrap();
        std::fs::write(dir.join("m.tsv"), "a.wav\thu\tEz egy mondat.\tcaedf283babc\tx\nb\thu\tKettő.\n").unwrap();
        let clips = read_manifest(&dir.join("m.tsv")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let refs: Vec<&str> = clips.iter().map(|c| c.reference.as_str()).collect();
        assert_eq!(refs, ["Ez egy mondat.", "Kettő."]);
    }
}
