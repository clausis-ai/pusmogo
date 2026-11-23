# polish-09: trained local models as a cleanup pass after Whisper

Up: [../README.md](../README.md)
Verified: 2026-11-23

Date: 2026-11-23. Question: does a small local language model, trained on the Hungarian speech model's own mistakes, clean its text without damage? **Answer: no. Nothing adopted.**

## Data

- Real pairs: hu-run1 over 22,958 unvoted Common Voice clips and 5,740 pseudo-whispered copies (its training had other clips; 84 % of the speakers are shared). 10,703 spoken and 4,122 whispered training pairs (`scripts/make-polish-pairs.py`), test sentences kept apart.
- Rule-made pairs: `scripts/make-polish-synth.py` (spelling slips, English terms) and `scripts/make-polish-rewrite.py` (fillers, repeats, spoken corrections), with clean sentences from `scripts/make-polish-text.py`.

## Results

Word errors before → after. Spoken: 300 test sentences, 127 already right; whispered: 200, 59 already right. The sets hold about 30 % right sentences; in all clips it is about 70 %. The models ran uncompressed in MLX with the GPU shared.

| model | training | spoken | right ones broken | whispered | misheard (55) | hand-written (40) | p50 |
|---|---|---|---|---|---|---|---|
| none | | 11.7 % | | 15.2 % | 11.6 % | 9.4 % | |
| Qwen3-1.7B, LoRA | 14,825 real + 8,000 rule-made pairs | 12.8 % | 14 of 127 | 15.4 % | 11.8 % | 4.5 % | 0.28 s |
| Apertus v1.1 0.5B, full | the same | 13.3 % | 25 of 127 | 16.7 % | 11.4 % | 7.2 % | 0.08 s |
| Apertus v1.1 0.5B, 4-bit | none, four examples in the prompt | 25.4 % | 15 of 127 | 25.5 % | 75.5 % | 16.2 % | 0.06 s |
| Apertus v1.1 1.5B, 4-bit | none, four examples in the prompt | 24.6 % | 14 of 127 | 25.2 % | 25.0 % | 36.6 % | 0.07 s |

Two earlier versions trained on rule-made pairs only (Qwen3-0.6B, Qwen3-1.7B) fixed 0 of 39 misheard items by exact match; the second changed an amount ("80, javítom, 90 ezer" → "8,0 ezer").

With a guard (the model proposes; only punctuation, case and spelling of words unknown to Hunspell are accepted; chosen on a validation set) Apertus 0.5B gives spoken 11.73 → 11.48 %, 6 better and 1 worse, no right sentence broken, whispered 15.19 → 15.25 %. Exactly right sentences: 82 → 90, 47 → 50, misheard 16 → 14, hand-written 10 → 15.

Spell-check rules without a model (Hunspell hu_HU, sound-alike spellings with one known candidate, lower-case words, rules chosen on one part of the speakers): on the other part spoken 7.942 → 7.895 %, 16 better, 0 worse, none of 2,915 right ones broken; whispered 20.699 → 20.687 %. Safe and tiny.

The core now does this (`spell.rs`, the dictionary in Rust through spellbook, which agrees with Hunspell on 99.6 % of 48,656 words). Same rules, Rust code: other speakers spoken 7.942 → 7.904 %, 13 better, 0 worse, none of 2,915 right ones broken; whispered 20.699 → 20.687 %, 1 better, 0 worse. On the speakers the rules were chosen on: 70 better, 6 worse, 4 of 13,088 right ones broken. First use 57 ms, then 0.03 ms per dictation, about 22 MB while the engine is loaded, 0.5 MB in the app. macOS's own spell checker with the same rules: 11 better, 4 worse, 2 right ones broken.

Fine-tuning Apertus in bf16 gives nan gradients in the first layer; float32 works.

## What the mistakes are

Spoken, 22,652 clips (those above 50 % WER left out): 6.3 % of words wrong. Of the mistakes 28.5 % are one word against two, 28.0 % names, 19.8 % a common word turned into a non-word, 14.6 % a common word turned into another word, 7.2 % rare or foreign words. Among 16,003 sentences with every word right, 50 differ only in commas and 116 only in the end mark.

New sentence and new speaker: 10.0 % WER (3,606 clips); all clips 6.8 %.

## Findings

1. A model that reads only text repairs less than it damages at these sizes.
2. Misheard words need better hearing: the next speech fine-tune, not a text model.
3. `pusmogo-bench` read a fourth manifest column as reference text. Fixed here; run 1 and its published numbers used manifests without it.
