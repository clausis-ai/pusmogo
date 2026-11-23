# prompt-03: context hints only for dictations of 2 s or more

Up: [../README.md](../README.md)
Verified: 2026-11-08

[prompt-01](prompt-01.md) showed that English context hints (words from a terminal or a chat around the cursor) push short Hungarian dictations toward English respellings, and [prompt-02](prompt-02.md) won most of that back with a Hungarian carrier sentence (short-clip WER with terminal hints 36.2 %, no prompt 35.1 %). This change leaves the context hints out of the prompt when the dictation holds less than 2 s of voiced audio (`context::MIN_VOICED_SECS_FOR_HINTS`, measured with the energy detector `vad::VoiceActivity`). The personal dictionary stays in the prompt at every length, and `context::respell` still fixes near-miss spellings of hinted names in every dictation.

## Why 2 s

Voiced time per clip, same detector the app uses:

| set | voiced seconds |
|---|---|
| short phrases, `short/{hu,noisy}` (88 clips) | median 0.87, 90th percentile 1.86, 1 clip ≥ 2.0 |
| name sentences from [context-01](context-01.md) (8 clips) | 2.16–2.79 |
| FLEURS HU (100 clips) | 2.58 and up |

At 2 s every name sentence and every FLEURS clip keeps its hints, so their outputs cannot change; nearly every short phrase loses them.

## Result

| condition | short all | exact (of 132) | empty | names (8) | FLEURS |
|---|---|---|---|---|---|
| no prompt | 35.1 | 69 | 4 | | |
| terminal hints, before (7a9526c) | 36.2 | 63 | 7 | 0.0 | 17.0 |
| terminal hints, threshold 1.5 s | 36.2 | 68 | 5 | 0.0 | |
| terminal hints, threshold 2.0 s (shipped) | 35.5 | 70 | 5 | 0.0 | unchanged (0/100 outputs differ) |
| chat hints, before (7a9526c) | 37.3 | 64 | 4 | | 17.2 |
| chat hints, threshold 2.0 s (shipped) | 35.5 | 70 | 5 | | |

WER %, short = `bench/audio/short/{hu,low,noisy}.tsv` (132 clips), large-v3-turbo q5_0, beam search, Silero speech detector on, Apple M5 Pro, `pusmogo-bench --context-file`. Synthetic speech (macOS TTS) is easier than a real voice.

## Findings

1. **Short-clip WER with English hints is now within half a point of no prompt** (35.5 vs 35.1 %), down from 36.2–37.3 %, which meets the prompt-01 target (≤ 36.1 %). The terminal and chat conditions now give the same outputs, because only one short clip still gets hints.
2. **Names keep their spelling**: the name set stays at 0.0 % WER, every clip there has more than 2 s of voiced audio.
3. **FLEURS is unchanged by construction**: the same binary with the threshold at 0 (hints always on) gives identical outputs on all 100 clips.
4. **Bench order matters.** The remaining 6 differences from the no-prompt run are noisy clips after the one clip that still got hints, both better and worse ("Hol a lángfa?" → "Hol a lámpa?", "Kregágyukkor" → empty). Whisper state is not fully reset between calls, so one prompted pass changes later ones. FLEURS run alone differs on 41 of 100 outputs from FLEURS run after the short sets with the same prompt (17.0 vs 16.7 % WER). Compare outputs only between runs with the same manifests in the same order.

## Still open

A real voice speaks slower than TTS, so a spoken short phrase may cross 2 s more often. The dictation record (`keepDebugAudio`) shows the hints each dictation got; use it on the next real-voice test. Dropping plain English words (Settings, Monday) from hints when dictating Hungarian is no longer needed for short clips; for longer ones FLEURS shows no cost.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
S=bench/audio/short
core/target/release/pusmogo-bench --json term.json --context-file term.txt $S/hu.tsv $S/low.tsv $S/noisy.tsv
PUSMOGO_HINT_MIN_VOICED=0 core/target/release/pusmogo-bench ...   # hints at every length, as before
```

The context files are the ones listed in [prompt-01](prompt-01.md).
