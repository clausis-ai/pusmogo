# "Köszönjük!" over noise (noise-05)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Without the speech detector, Whisper wrote "Köszönjük!" ("Thank you", plural) over 4 s of key clicks and of chatter (`noise-04.md`). The singular "Köszönöm" was already on the known-phrase list in `vad.rs`; the plural was not, so the low-confidence rule never applied to it.

## Measured

Mean token probability of each "Köszönjük…" segment, with a temporary per-segment trace:

| audio | voiced s | confidence |
|---|---|---|
| 4 s key clicks, detector off | 0.69 | 0.674 |
| 4 s chatter, detector off | 2.88 | 0.639 |
| spoken "Rossz a hang." over key clicks, detector on | 1.02 | 0.501 |
| spoken "Köszönjük…" (4 phrases × clean, lower voice, fan, chatter, keys) | 0.39–1.80 | 0.874–0.992 |

The rule that already drops a known phrase below 0.75 separates the two groups with a wide margin.

## Change

`köszönjük`, `köszönjük szépen` and `köszönjük a figyelmet` join the known phrases, and `köszönjük` joins the openings, so "Köszönjük, hogy…" sentences invented over noise get the same check as "Köszönöm, hogy…".

## Gate

Old and new release `pusmogo-bench`, same clip lists in the same order.

Detector on, 396 clips (short hu/low/noisy, FLEURS hu 100, synth hu/mixed/en/noisy, silence, noise, noise-long, plus 20 spoken "Köszönjük" clips): **1 output changed**. `short-noisy030-keys` (reference "Rossz a hang.") went from the invented "Köszönjük!" to empty. FLEURS, the synth sets and all 20 spoken "Köszönjük" clips gave the same output.

Detector off (`PUSMOGO_SPEECH_DETECTOR=0`), 78 clips (silence, noise, noise-long, short noisy, spoken "Köszönjük"): **2 outputs changed**, `noise-keys-4s` and `noise-babble-4s`, "Köszönjük!" → empty. Silence and noise sets now give no text with the detector off as well.

large-v3-turbo q5_0, Apple M5 Pro, macOS TTS (Tünde) mixed with synthetic room sounds. Synthetic speech is easier than a real voice.

## Limits

For `short-noisy030-keys` the user now gets nothing instead of a wrong phrase. Both are failures; this one does not type words the user never said. A real "Köszönjük." said quietly or far from the mic could fall under 0.75 and be dropped; the lowest spoken score here was 0.874, over chatter.

## Reproduce

Render the spoken clips with `say -v Tünde` (and the noisy variants the way `scripts/render-short.py` mixes them) into a manifest, then:

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
target/release/pusmogo-bench --json /tmp/new.json ../bench/audio/short/{hu,low,noisy}.tsv \
  ../bench/audio/fleurs-hu/hu.tsv ../bench/audio/synth/{hu,mixed,en,noisy,silence,noise,noise-long}.tsv kj.tsv
PUSMOGO_SPEECH_DETECTOR=0 target/release/pusmogo-bench --json /tmp/off.json \
  ../bench/audio/synth/{silence,noise,noise-long}.tsv ../bench/audio/short/noisy.tsv kj.tsv
```
