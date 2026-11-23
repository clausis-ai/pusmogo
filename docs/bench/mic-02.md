# mic-02: does the capture path damage the signal?

Up: [../README.md](../README.md)
Verified: 2026-11-08

A first test with a real voice on the built-in microphone (48 kHz mono) gave poor Hungarian ("mi a lopás vs rablás" came out "Mi a López versus Sablás?") while TTS clips of the same text were near perfect. This run checks whether the app damages the audio between the microphone and the engine. It does not: the conversion is transparent for recognition, and the only loss is the first ~90 ms after the key goes down.

## Format conversion (offline)

`scripts/mic-path-check.swift <16 kHz WAV> [rate] [channels] [chunk] [out.wav]` upsamples a clip to a device format with a one-shot, highest-quality converter, cuts it into tap-sized buffers and converts it back with the same calls as `AudioRecorder.append` (one `AVAudioConverter` per recording, one buffer per call).

| Device format, buffer | Length change | Lag | Level (rms) |
|---|---|---|---|
| 48 kHz × 1, 4800 frames | −10.4 ms (tail held in the converter) | 0 | 0.0794 → 0.0785 |
| 48 kHz × 1, 1024 frames | −0.4 ms | 0 | 0.0793 → 0.0784 |
| 44.1 kHz × 1, 4410 frames | −1.7 ms | 0 | 0.0793 → 0.0785 |
| 48 kHz × 2, 4800 frames | −10.4 ms | 0 | 0.0794 → 0.0785 |

No clipping, no gain change, no shift, nothing lost at the start; only up to ~10 ms of the tail stays in the converter, after the user has stopped speaking. Since then `AudioRecorder.stop()` drains the converter with an end-of-stream call, and the script does the same (`--no-drain` gives the old behaviour): all four rows above now come back with exactly the source length (hu001: 55166 samples in and out; −220 samples without the drain at 4800-frame buffers). The waveform difference (about 22 dB below the signal on a synthetic clip) is the same against a one-shot highest-quality conversion of the same device signal, so it is the band edge of any 48 → 16 kHz round trip, not the app.

What matters is what Whisper hears. Every clip of two sets went through the 48 kHz / 4800-frame path and was transcribed again (`pusmogo-bench`, large-v3-turbo q5_0, Apple Silicon):

| Set | Original WER | Through the app path | Outputs changed |
|---|---|---|---|
| FLEURS HU (100 real voices) | 17.4 % | 17.6 % | 13 / 100, single words both ways |
| Synthetic HU (60, Tünde) | 7.3 % | 7.3 % | 0 / 60 |

## Start of the recording (live, built-in microphone)

`PUSMOGO_MIC_TEST=2` now logs when the first sample of the recording was captured (the first buffer's host time) and how much digital silence it starts with. Five runs on the built-in microphone (48000 Hz × 1, system default): first audio 90–91 ms after `start()`, 0 ms of digital silence, every run. Each dictation now records the same figure as `micStartMs` in `~/Library/Logs/Pusmogo/timings.jsonl`.

Speech in those first ~90 ms after key down is not recorded. It can cost the first sound of a word only when speaking starts together with the key press; the start sound comes at 180 ms. It does not explain a misheard consonant in the middle of a phrase ("rablás" → "Sablás").

## So the gap is elsewhere

The mic path is ruled out. The remaining suspects, in the order of the P0 plan: short clips (decoding without beam search or temperature fallback), the prompt and context hints, the model, and a real voice in a real room being harder than TTS. The recordings kept with `keepDebugAudio` (see the app's `AGENTS.md`) are the exact audio the engine received, so a bad dictation can now be replayed with the bench.
