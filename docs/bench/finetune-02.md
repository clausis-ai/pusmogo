# finetune-02: can the Hungarian turbo fine-tune's defects be fixed on our side?

Up: [../README.md](../README.md)
Verified: 2026-11-08

[finetune-01](finetune-01.md) found an MIT Hungarian turbo fine-tune that matches large-v3 on FLEURS at turbo's cost, but with three defects: a stray leading "." or "–" on a third of its outputs, no text at all for 7 of 132 short clips, and whole sentences skipped in 2 of 8 long dictations. This report traces each one to its cause and tries two decoder-side fixes. Measurement only. No product code changed, and the experimental patch was not merged.

**Answer: partly.** The empty one-word clips and the stray punctuation can be fixed on our side. The skipped sentences cannot be fixed with what we have: they come from the model itself, and no cheap check catches them. The fine-tune is still not a replacement for large-v3 on long dictations.

## Method

Same release `pusmogo-bench`, same eight manifests in the same order as finetune-01 (FLEURS HU, short hu/low/noisy, silence, noise, noise-long, long). A temporary trace, enabled by an environment variable, printed every raw segment the engine returned: text, token ids and probabilities, voiced seconds, confidence, and whether the noise guard dropped it. The trace run reproduced finetune-01's outputs exactly (254 of 254 clips identical).

## Findings

1. **Empty clips come from the model, not from our guards.** For all 7 empty clips the model decodes one segment holding only "." and then the end token (confidence 0.52–0.93). No guard dropped anything. Cleanup then turns a lone "." into nothing. Run on their own, the same clips give text (", Holnap.", ", Brablás."), so the Whisper state left by earlier clips decides which way it goes.
2. **The stray leading punctuation is a low-probability first token.** Outputs open with "." "," or " –" at a probability of only 0.05–0.09, followed by the real first word.
3. **Banning punctuation as a segment's first token makes things worse.** A logits filter that forbids punctuation-only tokens before any text (opening quotes and brackets still allowed) removes every stray mark. It also makes the model guess a filler first word, mostly "A", which replaces the real one ("Például a tanulást" → "A tanulást", "Ha télen" → "A télen"). FLEURS WER goes from 13.6 to 17.9 %, short HU from 18.5 to 43.5 %, long from 19.5 to 27.9 %. Rejected.
4. **Forbidding the end token straight after punctuation-only text fixes the empty clips.** With this filter, all 7 formerly empty clips give text: 2 exactly right ("Rablás.", "Holnap." apart from a dash), the rest near misses ("Brablás.", "Lise-t.", "Hall!"). It costs one more invented text over music: 2 of 4 noise beds produce text instead of 1 ("A személye, a személye és a személye…"). A control run with the filter installed but doing nothing gave the same outputs as no filter at all.
5. **The skipped sentences start where a 30 s window opens with the stray ".".** Long dictations are cut at pauses into windows of up to 30 s, each decoded as one segment. In `long-04` the third window opens with "." (probability 0.07) and goes straight to the "Izrael …" sentence, so the NHK sentence before it is never typed. In `long-06` the second window opens with "." and goes straight to "Ez lett …", leaving out the first half of the sentence before it. The density check that catches skips under a prompt (fewer than 9 characters per voiced second) misses both: the first window has 9.4, just above the threshold, and the second 14.4, normal for speech. Banning the opening "." (finding 3) does not bring the text back.

| set | finetune-01 WER / CER % | end-token filter WER / CER % | non-empty |
|---|---|---|---|
| FLEURS HU (100) | 13.6 / 3.3 | 13.8 / 3.4 | 100 → 100 |
| short hu (44) | 18.5 / 9.1 | **15.2 / 5.3** | 41 → 44 |
| short low (44) | 15.2 / 5.7 | 17.4 / 5.5 | 43 → 44 |
| short noisy (44) | 34.8 / 12.7 | 33.7 / 10.6 | 41 → 44 |
| silence + noise (10) | no text | no text on silence; **2 of 4 noise beds with text** (was 1) | |
| long (8) | 19.5 / 9.4 | 19.1 / 9.3 | sentences still skipped |

For comparison, large-v3 in finetune-01: FLEURS 13.4 %, short hu 18.5 %, short noisy 45.7 %, long 14.1 / 4.0 %.

## What it means

- **For one-pass dictations (under 30 s)** the fine-tune with the end-token filter is as accurate as large-v3 or better, at turbo's latency and memory. Stripping a leading "." or dash in cleanup would remove the stray punctuation. Inventing text over music still needs a guard, and noise-long was never tuned for this model.
- **For long dictations** it loses text silently: in 2 of 8 clips the text of a whole sentence is missing. Nothing in the decoded text gives this away, so the app cannot notice it and recover.
- A split setup (fine-tune for short dictations, large-v3 for long ones) would need both models loaded or a swap in the middle of a dictation: about 2.9 GB of memory, or a reload while the user is speaking. Not attractive for v0.1.
- The default stays large-v3. Whether to pursue the fine-tune further (for example only on Macs with 8 GB, where turbo is the default today) is a decision for the maintainers.

## Reproduce

The patch (a whisper.cpp logits filter set through `FullParams::set_filter_logits_callback`, about 60 lines in `engine.rs`) was not committed. Its two filters are:

- ban: if no text token has been decoded in the segment yet, set the logits of every punctuation-only token to −∞;
- end: if every text token decoded so far is punctuation-only, set the end token's logit to −∞.

Then run the command from [finetune-01](finetune-01.md#reproduce) with the same manifests and compare per-clip `output` fields.
