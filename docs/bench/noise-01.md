# Room noise (noise-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

The hallucination guard ([guard-01](guard-01.md)) was tuned on digital silence, steady noise and a hum. A real microphone also hears typing, music and people talking. Those sounds have loudness swings like speech, so they pass the energy check, and Whisper then writes text over them.

## New bench sets

`scripts/render-bench.py` now also writes, deterministically:

- `noise.tsv`: 4 s of fan (brown noise, −35 dBFS), typing (4 ms clicks every 90–250 ms), music (plucked chord notes every 250 ms, −30 dBFS) and babble (noise with a 4 Hz syllable envelope, −32 dBFS). Empty references: any text is wrong.
- `noisy.tsv`: synth hu001–hu005 over each of those sounds (speech at −6 dB, the sound starting 0.5 s before the voice and running on after it). The text must survive.

## Finding

| clip | before | after |
|---|---|---|
| noise-fan-4s | (empty, energy check) | (empty) |
| noise-keys-4s | Köszönöm. | (empty) |
| noise-music-4s | Köszönöm, hogy megtaláltad! | (empty) |
| noise-babble-4s | Köszönöm. | (empty) |

No speech clip over noise ever went empty, before or after (noisy: WER 9.0 %, joined 6.2 %; babble is the hardest).

`no_speech_probability` from whisper.cpp read 0.000 for every segment in this setup, so it cannot tell noise from speech. The mean probability of a segment's text tokens can:

| text | audio | mean token probability |
|---|---|---|
| Köszönöm. / Köszönöm, hogy megtaláltad! | typing, music, babble | 0.60–0.69 |
| Köszönöm. / Köszönöm szépen. / Köszönöm, hogy megtaláltad. / Thank you. | spoken (TTS) | 0.93–0.98 |
| ordinary sentences over babble | TTS + noise | 0.77–0.96 |

## Change

`vad::drop_segment` (`core/crates/pusmogo-core/src/vad.rs`): a segment is dropped when it is a known silence phrase over under 0.12 s of voiced audio (as before), or when it is a known phrase or starts like one ("köszönöm", "thank you", "thanks for") and its mean token probability is below 0.75. Ordinary text is never dropped, however unsure the model was.

## Gate

Old and new release `pusmogo-bench` on the same 244 clips (noise, silence, spoken "köszönöm"/"thank you", noisy, synth hu/mixed/en, FLEURS hu 100): exactly 3 outputs differ, the three noise clips above, all now empty. No clip with a reference went empty. Latency unchanged (p50 0.68–0.69 s). In the app, `PUSMOGO_SELFTEST_WAV` with the typing clip gives "Nem hallottam semmit" and a spoken "Köszönöm." is still typed.

## Limits

TTS and synthetic noise, not a real room. A real "köszönöm" mumbled under loud music could fall under 0.75 and be dropped; the phrase list keeps that risk to a few words. Extend `HALLUCINATIONS` / `HALLUCINATION_OPENINGS` when the bench shows a new invented phrase.
