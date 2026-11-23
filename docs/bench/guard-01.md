# Hallucination guard (guard-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

Whisper writes text on silence: "NAMASTE" on digital silence and hum, "Köszönöm." on noise (6/6 silence clips in [synth-01](synth-01.md)), and "NAMASTE" on the 0.2 s tail window of a 30.2 s FLEURS clip ([fleurs-01](fleurs-01.md)). In the app, pressing the key without speaking would type that text.

## What changed

`core/crates/pusmogo-core/src/vad.rs`, used by the Whisper engine:

1. **Energy check before inference.** 30 ms frames; a frame is voiced when it is 15 dB above the clip's 10th-percentile floor (and above −70 dBFS), or 6 dB above the floor and louder than −40 dBFS. Under 0.12 s of voiced audio, the engine returns empty text without running the model. The first rule catches quiet microphones; the second catches fluent speech with no pause, where the loudness swing is only about 13 dB. Steady noise and hum vary by under 1 dB per frame, so they fail both rules.
2. **Segment filter.** A segment whose text is a known silence phrase ("namaste", "köszönöm", "thank you", ...) is dropped when its time span holds under 0.12 s of voiced audio. A user who really says "köszönöm" keeps it.

A first version used only the 15 dB rule and dropped two fluent TTS clips (hu038, mixed020) to empty text; the bench caught it and the second rule fixed it.

## Results (turbo q5, synthetic TTS speech and FLEURS real speech)

| set | before WER % | after WER % | after WER joined % |
|---|---|---|---|
| synth hu (60) | 7.0 | 7.0 | 5.7 |
| synth mixed (30) | 19.2 | 19.2 | 19.2 |
| synth en (20) | 0.0 | 0.0 | 0.0 |
| silence (6) | 6/6 hallucinated | 0/6, all empty, no inference | |
| FLEURS hu (100) | 17.5 | 17.4 | 13.9 |

Warm latency is unchanged (p50 0.72 s synth). Silent clips now return in well under 10 ms instead of ~1.1 s.

## Limits

- The thresholds are tuned on TTS, synthetic noise and FLEURS; real microphones in noisy rooms are untested. A modulated noise (music, chatter) can pass the energy check; then only the segment filter applies.
- The phrase list holds whole-utterance outputs seen in practice. Extend it when a new one shows up in the bench.
