# Longer room noise (noise-02)

Up: [../README.md](../README.md)
Verified: 2026-11-07

[noise-01](noise-01.md) fixed 4 s of room noise. A user who holds the key and then thinks for a while gives Whisper much more noise than that, so this round uses 15 s beds.

## New bench set

`scripts/render-bench.py` now also writes `noise-long.tsv`: the same fan, typing, music and babble sounds as `noise.tsv`, 15 s each, empty references.

## Finding

Batch and streaming (`--stream`) gave the same outputs. Each was one segment spanning the whole clip:

| clip | output | voiced audio under it | mean token probability |
|---|---|---|---|
| noise-fan-15s | Re | 0.39 s | 0.00 |
| noise-keys-15s | NAMASTE | 2.61 s | 0.76 |
| noise-music-15s | NAMASTE | 8.40 s | 0.79 |
| noise-babble-15s | Közöszönöm... | 10.92 s | 0.32 |

For comparison, over 284 real speech segments (synth HU, mixed and EN, FLEURS HU, the long set) the lowest mean token probability was 0.48 (a FLEURS sentence Whisper got mostly wrong), the next 0.69. Short known phrases really spoken (TTS) over typing, music or chatter measured 0.78 to 0.99 with at most 1.41 s of voiced audio under them. Confidence alone cannot separate "NAMASTE" over noise (0.76 to 0.79) from "Thank you." spoken over typing (0.78); the amount of voiced audio can.

## Change

`vad::drop_segment` (`core/crates/pusmogo-core/src/vad.rs`) adds two rules to the ones from noise-01:

- any segment with a mean token probability below 0.40 is dropped (`MIN_ANY_CONFIDENCE`), whatever it says;
- a known phrase ("NAMASTE", "Köszönöm.", "Thank you.") over more than 2 s of voiced audio needs a probability of 0.85 (`MAX_PHRASE_SECS`, `MIN_PHRASE_CONFIDENCE`).

## Gate

Old and new release `pusmogo-bench` on the same 244 clips (noise, noise-long, silence, noisy, synth hu/mixed/en, FLEURS hu 100): exactly 4 outputs differ, the four 15 s noise clips above, all now empty. No clip with a reference went empty. Streaming on noise-long, noisy and the long set: noise-long all empty, nothing with a reference empty. 16 spoken phrase clips (Köszönöm, Köszönöm szépen, Köszönöm a figyelmet, Thank you, Thank you very much, Bye, some over typing, music and babble): identical outputs before and after.

A first version that asked 0.85 of every known phrase dropped a spoken "Thank you." over typing and chatter and a clean "Bye."; the voiced-audio condition fixed that.

## Limits

TTS and synthetic noise, not a real room. The 0.40 floor sits close to the lowest real segment (0.48): a very unclear real sentence could be dropped where it used to be typed wrong. Inference still runs on noise-only clips (about 0.65 s for 4 s, 1.2 s for 15 s) because the energy check lets them through.
