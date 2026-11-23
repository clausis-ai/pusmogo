# beam-01: beam search for final passes

Up: [../README.md](../README.md)
Verified: 2026-11-08

Short Hungarian dictations come out wrong far more often than full sentences ([short-01](short-01.md)). Latency was flat at about 0.6 s, so there was room for a costlier decoding. This report compares greedy decoding (the old default) with beam search, width 5, in whisper.cpp.

## Result

Final passes shorter than 20 s now use beam search with 5 beams. Longer passes and caption previews stay greedy. `PUSMOGO_BEAM=1` restores greedy decoding for A/B runs.

| set | clips | greedy WER % | beam 5 WER % | greedy p50 s | beam 5 p50 s |
|---|---|---|---|---|---|
| short hu | 44 | 30.4 | 31.5 | 0.59 | 0.61 |
| short low | 44 | 28.3 | 23.9 | 0.59 | 0.62 |
| short noisy | 44 | 53.3 | 50.0 | 0.59 | 0.62 |
| short, all | 132 | 37.3 | 35.1 | 0.59 | 0.62 |
| FLEURS hu | 100 | 17.4 | 17.0 | 0.68 | 0.74 |
| FLEURS hu, streamed | 100 | 17.5 | 17.1 | 0.68 | 0.78 |
| synth hu | 60 | 7.3 | 6.4 | 0.62 | 0.69 |
| synth mixed | 30 | 18.7 | 18.3 | 0.61 | 0.68 |
| synth en | 20 | 0.0 | 0.0 | 0.64 | 0.67 |
| synth noisy | 20 | 9.0 | 9.0 | 0.68 | 0.70 |
| synth silence + noise | 10 | no text | no text | | |
| long hu (52–76 s) | 8 | 18.4 | 18.4 | 2.31 | 2.35 |

large-v3-turbo q5_0, Apple M5 Pro, `pusmogo-bench`, Silero speech detector on. Synthetic speech (macOS TTS) is easier than a real voice. In streaming, the release wait behind a running pass went from a mean of 0.05 s (max 0.69 s) to 0.07 s (max 0.84 s).

## Findings

1. **Beam search helps on every set with real errors, at about 30–80 ms per pass**, well inside the 300 ms budget for this item. On FLEURS with beam at every length, 16 clips got better and 8 worse (WER 17.4 → 16.8 %).
2. **Beam search on near-30 s windows dropped a whole sentence.** On the long set with beam at every length, one window lost "A víz a védőgát felett … zúdul alá." (WER 18.4 → 19.1 %, CER 4.6 → 5.7 %). Skipped speech is worse than a misheard word, so passes of 20 s and more stay greedy. With that cutoff the long set scores the same as before, and no FLEURS or synth clip got shorter output.
3. **Short clips: 9 better, 5 worse, 22 changed with the same score.** Fixed: "Helló", "Kérek egy kávét", "Rögtön jövök", "Szia, hogy vagy?", and two noisy clips that were empty before. Worse: "Rablás" → "Rabláss", "Hétfő" → "Hét fő", "Hol a lámpa?" → "Hol a lángfa?" in noise. Three clips that were already wrong now come out empty (two low, one noisy). One noisy clip that was already wrong now reads "Köszönjük!", a common Whisper filler phrase. Empty outputs over the whole short set: 5 → 4.
4. **The short-phrase gap is not closed.** 35 % WER on short phrases against 6–17 % on sentences. Beam search is a small, cheap gain. The next steps are in the backlog: prompt audit, model, and short speech lost in noise.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
for b in 1 5; do PUSMOGO_BEAM=$b target/release/pusmogo-bench --json /tmp/b$b.json \
  ../bench/audio/short/{hu,low,noisy}.tsv ../bench/audio/fleurs-hu/hu.tsv ../bench/audio/long/hu.tsv; done
```
