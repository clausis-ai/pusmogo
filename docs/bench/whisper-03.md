# whisper-03: decoding levers on whispered Hungarian

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08 · Model: `ggml-large-v3-q5_0.bin` (the Hungarian default) · Binary: `pusmogo-bench` at 11f4209 plus temporary, uncommitted environment knobs · Sets: the whispered short (44) and whispered FLEURS (100) sets from [whisper-01](whisper-01.md)

## Why

[whisper-02](whisper-02.md) rejected an audio chain and left the decoding levers: beam width, temperature fallback, the no-speech threshold, a Hungarian prompt, the speech detector's threshold, and a check that our hallucination guard does not throw away correct whispered text. Each was measured on whispered input alone; a lever that won would then have been gated on the normal sets and paired with the whisper detector from whisper-02.

## Method

One knob at a time against the shipping defaults (beam 5 under 20 s, fallback on, speech detector on, no prompt with an empty dictionary). The knobs were temporary environment switches in `engine.rs`, removed again after the run:

- beam 8, and greedy (beam 1)
- temperature fallback off (`temperature_inc = 0`)
- no-speech threshold relaxed to 1.0
- the Hungarian prompt sentence "Ez egy magyar nyelvű jegyzet." (`--prompt`)
- Silero threshold 0.3 instead of 0.5, plus a log of its per-frame probabilities
- the hallucination guard logged, and bypassed

"Typed wrong" counts non-empty outputs with CER ≥ 60 %: text that would be typed and is not what was said.

## Results

WER / CER %, large-v3, detector on.

| lever | whispered short WER / CER | exact | empty | typed wrong | whispered FLEURS WER / CER | p50 s (FLEURS) |
|---|---|---|---|---|---|---|
| baseline | 87.0 / 84.7 | 2 | 21 | 16 | 42.3 / 19.3 | 1.37 |
| beam 8 | **118.5** / 121.0 | 2 | 21 | 17 | 41.4 / 19.2 | 1.93 |
| greedy | 89.1 / 87.3 | 2 | 21 | 15 | 43.8 / 20.3 | 1.18 |
| fallback off | 87.0 / 84.7 | 2 | 21 | 16 | 42.3 / 19.3 | 1.58 |
| no-speech 1.0 | 87.0 / 84.7 | 2 | 21 | 16 | 42.3 / 19.3 | 1.57 |
| Hungarian prompt | 95.7 / 97.0 | 1 | 26 | 14 | 42.2 / 19.1 | 1.51 |
| Silero 0.3 | 93.5 / 98.1 | 4 | 15 | 20 | 42.3 / 19.3 | 1.64 |
| guard bypassed | 88.0 / 87.3 | 2 | 20 | 17 | 42.3 / 19.3 | 1.72 |

Fallback off and the relaxed no-speech threshold gave byte-identical outputs on all 144 clips: neither path fires on these clips. Latency differences between rows other than beam 8 and greedy are run-to-run drift (the same settings ran at 1.37 and 1.58 s).

Speech detector probabilities (Silero, highest frame per clip): normal short phrases 0.99–1.00 on all 44; whispered short phrases median 0.79, min 0.17, and 34 of the 38 clips that pass the energy check have at least one frame over 0.5. The detector does hear whispers, only weaker and in shorter runs, so many fall under its minimum speech length.

## Findings

1. **No decoding lever helps whispered Hungarian.** Beam 8 makes short whispers much worse (longer invented text, 87.0 → 118.5 %) for a 0.9-point FLEURS gain at +0.56 s per pass. Greedy is worse on both sets. Fallback off and the no-speech threshold change nothing.
2. **The Hungarian prompt trades one wrong phrase for another.** On short whispers it empties 5 more clips and turns 4 others into "Köszönöm, hogy megnéztétek!" or a subtitle line; FLEURS is flat (42.3 → 42.2 %). The real whispered clip did not change with a prompt either.
3. **A lower detector threshold lets 6 more short whispers through, and 4 of those come out as stock phrases** ("NAMASTE", "Jó étvágyat kívánunk!", "Köszönöm, hogy megnéztétek!"); two come out right ("Haló, haló, haló, haló.", "Igen, igen, igen."). Net WER 87.0 → 93.5 %. This agrees with whisper-02: opening the gates for whispers brings stock phrases.
4. **The hallucination guard does not drop correct whispered text.** Across all 144 whispered clips it fired once, on the subtitle credit "Feliratok az Amara.org közösségétől", which is right to drop.
5. **W2 is closed with nothing adopted.** Neither the audio path (whisper-02) nor the decoder settings move whispered Hungarian; the model has to learn it. The whisper detector from whisper-02 is not built, since there is no lever for it to switch on. The next step is the fine-tune data prep (W3): pseudo-whispered and, later, real whispered recordings.
