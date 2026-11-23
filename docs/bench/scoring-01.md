# Scoring 01: normaliser fixes and compound-tolerant WER (2026-11-07)

Re-scores the [synth-01](synth-01.md) and [fleurs-01](fleurs-01.md) sets after two scoring changes in `pusmogo-bench`. The engine, model and cleanup are unchanged, so the difference is scoring only. Apple M5 Pro, `ggml-large-v3-turbo-q5_0.bin`, warm engine. Synthetic audio is macOS TTS, a proxy for real speech.

## What changed

1. **"-val/-vel" after numbers and percent.** Hungarian assimilates this suffix to the preceding consonant ("tizenöt százalékkal", "hárommal", "öttel"). The normaliser now maps every assimilated form, digit or word, to one canonical spelling, so "tizenöt százalékkal" and "15%-al" score equal. "15%-al" is itself misspelled ("15%-kal" is correct); fixing the output is a cleanup task (D2), not a recognition error, so the bench no longer counts it.
2. **New column, "WER joined".** Adjacent words on one side whose concatenation is a single word on the other side are merged before alignment ("marketing kampány" vs "marketingkampány", "közel merészkednek" vs "közelmerészkednek"). Plain WER is unchanged and stays the headline number; the gap between the two is the share of errors that are only word boundaries.

"nyolckor" vs "8-kor" (synth-01 finding 4) already scored equal; hu055's one error is "reptére" for "reptérre". A test now pins both.

## Results

| set | clips | WER % (before) | WER % | WER joined % | CER % |
|---|---|---|---|---|---|
| synth hu | 60 | 7.5 | 7.0 | 5.7 | 1.5 |
| synth mixed | 30 | 19.2 | 19.2 | 19.2 | 4.5 |
| synth en | 20 | 0.0 | 0.0 | 0.0 | 0.0 |
| FLEURS hu | 100 | 17.5 | 17.5 | 14.0 | 4.5 |

- hu014 and hu054 ("…százalékkal" vs "…%-al") go from 11.1 % and 12.5 % to 0 %.
- On real speech, **one fifth of the word errors (3.5 of 17.5 points) are compound splits or joins.** Those cost a reader little; the remaining 14 % is the number to move with engine and vocabulary work.
- Silence clips still score 100 % (hallucination, a separate item).

Latency is unchanged (synth p50 0.72 s, FLEURS p50 0.80 s).
