# Short words in noise: decoder settings (noise-06)

Up: [../README.md](../README.md)
Verified: 2026-11-08

`noise-04` turned empty outputs for single words spoken over noise into near misses ("RUKA" for "Róka", "Levil" for "Levél"). This report asks whether a decoder setting, rather than a different model, gets them right with large-v3-turbo. Result: no. None of the settings tried lowers WER on the short-phrase sets, so nothing changed in the app.

## What was tried

Each setting was added behind a temporary environment variable, run on the 132 short clips (`bench/audio/short/{hu,low,noisy}.tsv`, same order every run, speech detector on) and diffed per clip against the unchanged binary. Two baseline runs gave identical outputs (0/132 changed), so every difference below comes from the setting.

| setting | short all WER % | CER % | exact | clips changed | noisy WER % |
|---|---|---|---|---|---|
| current (beam 5, whisper.cpp temperature fallback) | 35.5 | 14.3 | 44 | n/a | 50.0 |
| temperature fallback off (`temperature_inc` 0) | 35.9 | 14.8 | 42 | 29 | 48.9 |
| fallback samples 5 candidates instead of 1 | 35.5 | 14.2 | 44 | 1 | 50.0 |
| same, fallback triggered earlier (log-prob threshold −0.5) | 37.0 | 14.0 | 43 | 32 | 52.2 |
| noise gate: frames more than 150 ms from voiced audio × 0.1 | 35.5 | 14.5 | 42 | 18 | 51.1 |
| same, × 0.3 | 37.0 | 14.8 | 42 | 29 | 52.2 |

"RUKA" became "Ruka" under three settings; "Levil" never changed. Every setting fixed some clips ("Hol a lángfa?" → "Hol a lámpa?") and broke others ("Kérek egy kávét." → "Kérek egy kávit.", "Hajó" → "Ha jó!") in about equal numbers.

## Findings

- **Under beam search the temperature fallback samples one candidate.** whisper.cpp uses `greedy.best_of` for passes at temperature > 0 even when the strategy is beam search, and the beam-search defaults leave it at −1, so each fallback pass draws a single sample. whisper-rs 0.15 has no setter for it. Raising it to 5 (scratch copy of whisper-rs) changed 1 of 132 clips: the fallback rarely decides the result on this set.
- **The fallback still matters.** Turning it off changes 29 clips, both ways. Its sampling state lives in the Whisper state, so one changed clip can change later clips that also fall back: compare per-clip outputs only between runs over the same clips in the same order.
- **A light noise gate does not help.** The noise that hurts sits under the word itself, where a gate cannot remove it.

The one lever measured that does fix these words is the model: large-v3 q5_0 gets "Róka" and "Levél" right and lowers short noisy WER 50.0 → 40.2 % (`bakeoff-02.md`).

large-v3-turbo q5_0, Apple M5 Pro, macOS TTS (Tünde) mixed with synthetic room sounds. Synthetic speech and noise are easier than a real voice in a real room.

## Reproduce

Add the setting to `FullParams` in `core/crates/pusmogo-core/src/engine.rs` (`set_temperature_inc`, `set_logprob_thold`; the candidate count needs a patched whisper-rs), then:

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
target/release/pusmogo-bench --json /tmp/new.json ../bench/audio/short/{hu,low,noisy}.tsv
```
