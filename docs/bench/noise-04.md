# Short words in noise (noise-04)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Single words and short phrases spoken over fan, typing or chatter at 10 dB SNR (`bench/audio/short/noisy.tsv`) sometimes came out empty: the user spoke, released the key and nothing was typed. This report finds the stage that dropped them and lowers the bar for short segments.

## Where the text went

With a temporary trace of every segment's text, voiced seconds and mean token probability:

- **Not the speech detector.** The noisy set gives the same outputs with Silero on and off (`PUSMOGO_SPEECH_DETECTOR=0`): Whisper ran on every empty clip.
- **Not beam search.** Greedy decoding (`PUSMOGO_BEAM=1`) left 3 of the 5 still empty.
- **The confidence floor.** Each empty clip was one segment below `MIN_ANY_CONFIDENCE` (0.40), dropped by `vad::drop_segment`. The dropped text was usually a near miss: "Levil" for "Levél" (0.35), "RUKA" for "Róka" (0.37), "Ha jól!" for "Hajó" (0.39), "Lánk hoz!" for "Lámpa" (0.34).

Text Whisper invents over noise that reaches it below 0.40: "Re" over 15 s of fan (0.00, 0.39 s voiced) and "Közöszönöm..." over 15 s of chatter (0.32, 10.9 s voiced, `noise-02.md`). With the speech detector on, only the music clips reach Whisper at all, and they decode as "NAMASTE" at 0.76–0.79, handled by the phrase rules.

## Change

`vad::drop_segment` keeps a segment with 0.12–2.0 s of voiced audio under it down to a probability of 0.30 (`MIN_SHORT_CONFIDENCE`); longer segments keep the 0.40 floor. A near miss can be corrected; an empty result looks like the app did not hear anything.

## Gate

Old and new release `pusmogo-bench`, same 386 clips in the same order (short hu/low/noisy, FLEURS hu 100, synth hu/mixed/en/noisy, silence, noise, noise-long), Silero on:

| clip | reference | before | after | CER before → after |
|---|---|---|---|---|
| short-low015 | Hajó. | (empty) | Ha jól! | 1.00 → 0.50 |
| short-low017 | Rizs. | (empty) | Réj! | 1.00 → 0.75 |
| short-noisy011-babble | Róka. | (empty) | RUKA | 1.00 → 0.25 |
| short-noisy012-keys | Levél. | (empty) | Levil | 1.00 → 0.20 |

No other output changed. Empty outputs on clips with speech: 4 → 0. Silence, noise and noise-long: no text before or after. FLEURS and the synth sets: identical. WER on the low set rises 23.9 → 25.0 % because "Ha jól!" is two words for one; CER over the run 4.3 → 4.2 %.

With Silero off (silence, noise, noise-long, short noisy): three noisy clips gain text ("RUKA", "Levil", "Foly vagy morsz!" for "Hol vagy most?"), no noise clip changes. That run also shows an older leak: without the detector, 4 s of typing and of chatter both come out as "Köszönjük!", which is not on the known-phrase list. It is in the backlog.

large-v3-turbo q5_0, Apple M5 Pro, macOS TTS (Tünde) mixed with synthetic room sounds. Synthetic speech and noise are easier than a real voice in a real room.

## Limits

The short-segment floor sits close to what chatter produced over long noise (0.32), but that case has far more than 2 s of voiced audio and stays on the 0.40 floor. The kept text is still wrong for all four clips: this turns "nothing" into "nearly right", it does not fix recognition of short words in noise.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
target/release/pusmogo-bench --json /tmp/new.json ../bench/audio/short/{hu,low,noisy}.tsv \
  ../bench/audio/fleurs-hu/hu.tsv ../bench/audio/synth/{hu,mixed,en,noisy,silence,noise,noise-long}.tsv
```
