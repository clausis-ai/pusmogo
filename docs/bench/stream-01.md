# Streaming sessions for long dictations (stream-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

**Result: adopted in the core (`session.rs`).** On one-minute dictations the wait after the key is released drops from 2.45 s to 0.80 s (p50) and from 3.16 s to 0.84 s (p95), at the same WER (18.7 %). Dictations under 29 s are transcribed exactly as before (FLEURS: 99 of 100 outputs identical, same WER).

## How it works

A `Session` receives audio while the user speaks. About once a second it checks whether at least 8 s are waiting and ends a chunk in the middle of the latest pause of 0.3 s or more (or at the quietest frame once 24 s pass without one), and transcribes that chunk in the background. On release, `finish()` makes one final pass over the tail **plus as many of the last chunks as fit in 29 s**: Whisper's cost is per 30 s window, so filling the window costs little, and the redone chunks give the tail its sentence context.

Two variants were measured and rejected on the long set below:

| variant | WER % | p50 s | p95 s |
|---|---|---|---|
| batch (no streaming), reference | 18.7 | 2.45 | 3.16 |
| chunks only, final pass = tail alone | 19.7 | 0.71 | 0.82 |
| + previous 40 words as the engine prompt | 21.3 | 0.71 | 0.84 |
| + tail under 8 s redone with the last chunk | 18.9 | 0.72 | 0.83 |
| **final pass fills 29 s (adopted)** | **18.7** | **0.80** | **0.84** |

The tail-alone variant lost words where a sentence was split at an inner pause: a 2 to 5 s tail ("amikor Jézus születését ünneplik") came out as noise ("A mi corjésus tőle tézsére unáplic"). Passing earlier text as the prompt made errors worse, not better.

## Setup

Turbo q5 (`ggml-large-v3-turbo-q5_0.bin`), Apple Silicon with Metal, warm engine, all runs in one session. `pusmogo-bench --stream` pushes each clip in 100 ms pieces and calls `step()` after every second of audio, as the app will while recording; the reported latency is `finish()` plus cleanup, the part the user waits for. Background steps took at most 0.86 s each, under the 1 s of audio between steps, so they keep up with speech.

- **Long set:** `scripts/make-long-set.py` joins five consecutive FLEURS Hungarian clips with 0.6 s gaps into 8 recordings of 58 to 76 s (524 s total), real speech with known references. Different speakers per recording, so it is a proxy for one person dictating.
- **Short set:** the 100 FLEURS Hungarian clips (2 to 30 s, `docs/bench/fleurs-01.md`).

## Results

| set | mode | WER % | WER joined % | CER % | p50 s | p95 s | empty |
|---|---|---|---|---|---|---|---|
| long (8 × ~65 s) | batch | 18.7 | 15.0 | 4.7 | 2.45 | 3.16 | 0 |
| long (8 × ~65 s) | stream | 18.7 | 15.3 | 4.4 | 0.80 | 0.84 | 0 |
| FLEURS (100) | batch | 17.5 | 14.3 | 4.5 | 0.73 | 0.80 | 0 |
| FLEURS (100) | stream | 17.5 | 14.3 | 4.5 | 0.73 | 0.79 | 0 |

The only FLEURS output that changed is the one clip over 29 s (30.2 s), with the same error count. Per long recording, errors batch → stream: 16→17, 22→19, 33→31, 11→11, 10→11, 15→19, 19→17, 11→12.

## Not done yet

The app still transcribes after release; wiring the session into `AudioRecorder`/`Core.swift` (push 100 ms pieces, step on the core queue once a second, finish on key up) is the next step. Background chunks finished before release are redone if they fall inside the final 29 s; that costs GPU time, not latency.
