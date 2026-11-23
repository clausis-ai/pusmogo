# Flash attention and thread count (flashattn-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

**Result: flash attention adopted, thread count unchanged.** Turning on whisper.cpp's flash attention makes warm transcription about 5 % faster with no accuracy change beyond run-to-run noise. The CPU thread count makes no measurable difference, because the encoder and decoder run on the GPU (Metal).

## Setup

Turbo q5 (`ggml-large-v3-turbo-q5_0.bin`), Apple Silicon with Metal, warm engine, both settings run back to back in one session with the same binary. Flash attention is a context parameter (`WhisperContextParameters::flash_attn`), off by default in whisper-rs 0.15.

## Synthetic TTS sets (116 clips, 2.5 to 4.6 s)

| setting | hu WER % | mixed WER % | en WER % | silence | p50 s | p95 s |
|---|---|---|---|---|---|---|
| flash attention off | 7.0 | 19.2 | 0.0 | 6/6 empty | 0.72 | 0.73 |
| flash attention on | 7.3 | 18.7 | 0.0 | 6/6 empty | 0.69 | 0.72 |

4 of 116 outputs changed: one Hungarian word got worse (hu035), one mixed clip got better (mixed029), two changed spelling without changing the score.

## FLEURS Hungarian (100 real-speech clips, 5.5 to 30.2 s)

| setting | WER % | WER joined % | CER % | p50 s | p95 s |
|---|---|---|---|---|---|
| flash attention off | 17.4 | 13.9 | 4.4 | 0.85 | 0.93 |
| flash attention on | 17.5 | 14.3 | 4.5 | 0.81 | 0.87 |

27 of 100 outputs changed, all small (single letters, spacing, numbers written as words); errors went both ways and no clip became empty. The +0.1 point is within the +0.5 point acceptance bar.

## Thread count

With flash attention on, Hungarian synthetic set, p50: 4 threads 0.75 s, 8 threads 0.76 s, 12 threads 0.75 s (2 threads: 0.76 s). The machine was warmer than in the runs above, so absolute numbers are higher; the point is that they do not move. The engine keeps `min(cores, 8)`.

## What this means

Short-clip latency is still dominated by the fixed 30 s encoder window (see [audioctx-01](audioctx-01.md)). Bigger wins need a different approach: keeping the engine warm (B1) and transcribing while the user is still speaking (B2).
