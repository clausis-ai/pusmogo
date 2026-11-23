---
name: bench
description: Measure Pusmogo's speech accuracy (WER/CER) and latency with the real Whisper model on manifests of WAV clips. Use before and after any change to the engine, decoding settings, prompts or cleanup.
---

# Run the speech benchmark

Up: [../../../AGENTS.md](../../../AGENTS.md)
Verified: 2026-11-08

1. **Model.** Default: `~/Library/Application Support/Pusmogo/models/ggml-large-v3-turbo-q5_0.bin` (download command in the root README). Override with `PUSMOGO_MODEL` or `--model PATH`.
2. **Build** (release, real engine; takes a few minutes the first time):
   ```bash
   cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
   ```
3. **Manifest.** A TSV with one clip per line: `id<TAB>lang<TAB>reference`, where `lang` is `hu` or `en` and `<id>.wav` (16 kHz mono) sits next to the manifest. Example: `bench/prompts/e2e.tsv`. Audio goes under `bench/audio/`, which git ignores.
4. **Synthetic clips.** The standard sets (60 Hungarian, 30 Hungarian with English terms, 20 English sentences in `bench/prompts/*.txt`, plus 6 silence clips, 4 room-noise clips and 20 speech-over-noise clips) render in about a minute with `scripts/render-bench.py`, into `bench/audio/synth/{hu,mixed,en,silence,noise,noisy}.tsv`. For a one-off clip:
   ```bash
   say -v Tünde -o x.aiff "Holnap délután háromkor találkozunk."
   afconvert -f WAVE -d LEI16@16000 -c 1 x.aiff x.wav
   ```
   English: `say -v "Eddy (English (US))"`. Synthetic speech is easier than real speech: say so next to any number you report.

   **Short phrases.** `scripts/render-short.py` renders 44 short Hungarian phrases (single words, "X vagy Y" questions, greetings) as `bench/audio/short/{hu,low,noisy}.tsv`, plus `app.tsv` through the capture conversion with `--app-path`. Gate any change aimed at short dictations on it (baseline `docs/bench/short-01.md`) and on FLEURS.

   **Real speech.** `scripts/fetch-fleurs.py` downloads the FLEURS Hungarian dev split once (~237 MB, cached outside the repo) and writes 100 clips plus `bench/audio/fleurs-hu/hu.tsv`. Baseline: `docs/bench/fleurs-01.md`. Run it for any change that claims a Hungarian accuracy gain. `--lang en` fetches 100 clips of the English test split (~290 MB) into `bench/audio/fleurs-en/en.tsv`; baseline `docs/bench/fleurs-en-01.md`.

   **Head-to-head.** `scripts/score-compare.py` scores 30 phrases (`bench/compare/phrases.tsv`) dictated into Pusmogo (kept recordings, replayed through turbo and large-v3) and any other app's text output in one table; procedure in `docs/bench/compare-README.md`, dry run with `tts` then `score --audio tts`.
5. **Run:**
   ```bash
   core/target/release/pusmogo-bench --md out.md --json out.json bench/audio/synth/{hu,mixed,en,silence,noise,noisy}.tsv
   ```
   Add `--stream` to run each clip through a streaming `Session` as the app does while recording (latency = the wait after release); long recordings for it come from `scripts/make-long-set.py` (`bench/audio/long/hu.tsv`, baseline `docs/bench/stream-01.md`). Add `--preview` for caption previews and `--release-after 0.3` to release 0.3 s after the last word, as a real key release does (`docs/bench/caption-01.md`).
   Add `--prompt-file F` (or `--prompt TEXT`) to test an engine prompt such as a vocabulary hint; gate it on FLEURS, not only on the mixed set (`docs/bench/vocab-01.md`). `--dict-file F` (one entry per line, `Spelling` or `spoken<TAB>Spelling`) runs a personal dictionary the way the app does, prompt fitted to 224 tokens (`docs/bench/dict-02.md`).
   To score another engine, write its raw output as `id<TAB>seconds<TAB>text` per clip and pass `--hyp-file F`: the text gets the app's cleanup and the same scoring, and no Whisper model loads (`docs/bench/bakeoff-04.md`).
   The tool warms the engine first (skip with `--no-warmup`), then reports WER, "WER joined" (compound splits and joins not counted, see `docs/bench/scoring-01.md`), CER, accent-folded CER, real-time factor and p50/p95 latency per language.
6. **Report.** Commit the markdown table under `docs/bench/` with the model, machine and date. Compare against the previous report there.

Pitfalls: latency drifts by about 30% if another heavy build runs at the same time, so time on an idle machine. Never commit audio of real people or model weights.
