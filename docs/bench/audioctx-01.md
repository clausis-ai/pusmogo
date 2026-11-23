# Shorter encoder window for short clips (audioctx-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

**Result: rejected.** Limiting Whisper's encoder window to the clip length makes short clips 2.2 times faster but costs 7 to 9 WER points on real Hungarian speech. The engine keeps the full 30 s window.

## Why we tried it

Whisper's encoder always processes a 30 s window (1500 frames, 50 per second of audio). A 4 s dictation pays for 26 s of padding, so warm latency is a flat ~0.72 s whatever the clip length ([synth-01](synth-01.md)). whisper.cpp's `audio_ctx` parameter limits the encoder to fewer frames.

## What we measured

Turbo q5 (`ggml-large-v3-turbo-q5_0.bin`), Apple Silicon with Metal, warm engine, idle machine. `ctx` = encoder frames used: clip frames + 64-frame margin, rounded up to 64, never below the floor; full window for clips near 30 s. "single" = whisper.cpp `single_segment`.

Synthetic TTS sets (hu 60, mixed 30, en 20; clips 2.5 to 4.6 s). TTS speech is easier than real speech.

| setting | hu WER % | mixed WER % | en WER % | p50 s |
|---|---|---|---|---|
| full window (current) | 7.0 | 19.2 | 0.0 | 0.72 |
| floor 384, single | 33.6 | 44.3 | 12.2 | 0.17 |
| floor 512, single | 7.7 | 21.9 | 0.5 | 0.22 |
| floor 512, single, no timestamps | 25.3 | 21.9 | 0.0 | 0.25 |
| floor 640, single | 7.3 | 19.2 | 0.0 | 0.30 |
| floor 768, single | 7.0 | 19.2 | 0.0 | 0.33 |
| floor 768, multi-segment | 7.0 | 19.2 | 0.0 | 0.39 |

FLEURS Hungarian dev, 100 real-speech clips (5.5 to 30.2 s, median 11.6 s):

| setting | WER % | WER joined % | p50 s | p95 s |
|---|---|---|---|---|
| full window (current) | 17.4 | 13.9 | 0.86 | 0.95 |
| floor 768, single | 26.5 | 23.3 | 0.45 | 0.78 |
| floor 768, single, margin 128 | 27.4 | 24.3 | 0.45 | 0.80 |
| floor 768, multi-segment | 26.5 | 23.3 | 0.42 | 0.72 |
| floor 1024, single | 24.4 | 21.1 | 0.56 | 0.80 |

## What goes wrong

The decoder does not stop at the end of the speech. It repeats the whole sentence ("Az űrben lévő műhold … a földre. Az űrben lévő műhold … a földre.") or appends an invented one. 33 of 100 FLEURS clips got worse and 18 better. With a 384-frame floor the same happens on the 3 to 4 s synthetic clips. A larger margin after the clip, multi-segment decoding and disabling timestamps did not fix it. The synthetic set at floor 768 looked perfect and would have hidden the regression: real speech is the gate for decoding changes.

## What next

Ideas for short-clip latency that keep the full window: flash attention in the Metal backend (`WhisperContextParameters::flash_attn`, off by default), thread count, and the streaming session (B2), which moves most of the work before key release. A shorter window could come back with a repetition guard, but only if FLEURS WER stays within 0.5 points.
