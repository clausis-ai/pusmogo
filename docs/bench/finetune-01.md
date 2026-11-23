# finetune-01: a Hungarian turbo fine-tune next to turbo and large-v3

Up: [../README.md](../README.md)
Verified: 2026-11-08

Pusmogo now uses large-v3 for Hungarian on Macs above 8 GB ([stream-03](stream-03.md)): more accurate than turbo, but twice as slow per pass and 2.1 GB of memory instead of 0.85 GB. This report asks whether an open Hungarian fine-tune of the turbo model gives large-v3's accuracy at turbo's cost. Measurement only; no product code changed.

**Answer: on accuracy, nearly yes; as it stands, it cannot ship.** On real read speech (FLEURS) and short phrases the fine-tune matches large-v3 at turbo speed and memory, and it is clearly the best of the three in noise. But it starts most outputs with a stray "." or "–", gives no text at all for some clean one-word dictations, skips whole sentences in long dictations, and invents a sentence over music.

## Candidates found

Searched Hugging Face for Hungarian Whisper fine-tunes on 2026-11-08 (turbo- and large-v3-based first).

| model | base | licence | weights | status |
|---|---|---|---|---|
| `sarpba/whisper-hu-large-v3-turbo-finetuned` (+ `-GGML`) | turbo | MIT | ready ggml q5_0, 574 MB | **measured here** |
| `Trendency/whisper-large-v3-hu` | large-v3 | Apache-2.0 | float32 safetensors, 6.2 GB | not run: download alone exceeds the run's disk budget |
| `benmajor27/whisper-large-v3-hu_full` | large-v3 | Apache-2.0 | float32, 6.2 GB | not run: same reason |
| `alphaedge-ai/whisper-large-v3-turbo-hun-32768` | turbo | Apache-2.0 | 1.6 GB | not a fine-tune: vocabulary trimmed to 32 768 tokens, same weights otherwise |
| `treebron/…`, `MrZalan/…`, `aronpaljakab/…` | large-v3 / turbo | none stated | | skipped: no licence, so no right to redistribute |
| base, small and large-v2 fine-tunes | | | | out of scope (smaller or older than turbo) |

The measured model's card states MIT, a fine-tune of `openai/whisper-large-v3-turbo` (MIT) on an undocumented "custom dataset" (its own evaluation: 7.5 % WER on that data). The GGML repo publishes q8_0/q5_0/q4_0 conversions; the q5_0 file used here matches its published SHA-256 prefix (`c7aa76beaa6bc503…`). Because the training data is not documented, FLEURS may overlap with it; the short and noise sets are synthetic clips made for this project and cannot.

## Result

Same release `pusmogo-bench` binary, same manifests in the same order, current decoding (beam 5 under 20 s, Hungarian carrier prompt, Silero speech detector on, noise guards from [bakeoff-05](bakeoff-05.md)). Long set in batch mode. Peak memory is the bench process's `peak memory footprint` from `/usr/bin/time -l` over the whole run.

| set | clips | turbo WER / CER % | **fine-tune** WER / CER % | large-v3 WER / CER % | p50 s turbo / ft / large |
|---|---|---|---|---|---|
| FLEURS HU | 100 | 17.0 / 4.1 | **13.6 / 3.3** | 13.4 / 3.5 | 0.82 / 0.82 / 1.55 |
| short hu | 44 | 30.4 / 9.1 | **18.5** / 9.1 | 18.5 / 5.3 | 0.68 / 0.67 / 0.89 |
| short low (quiet) | 44 | 26.1 / 8.3 | **15.2** / 5.7 | 13.0 / 3.6 | 0.69 / 0.67 / 0.88 |
| short noisy | 44 | 50.0 / 25.9 | **34.8 / 12.7** | 45.7 / 21.9 | 0.70 / 0.67 / 0.89 |
| silence + noise | 10 | no text | no text | no text | |
| noise long (15 s) | 4 | no text | **text on 1 of 4** | no text | |
| long (~45–60 s) | 8 | 18.4 / 4.7 | 19.5 / **9.4** | 14.1 / 4.0 | 2.68 / 2.58 / 4.96 |
| peak memory | | 867 MB | **865 MB** | 2129 MB | |

Apple M5 Pro with Metal. The short and noise sets are macOS TTS (Tünde), a proxy for a real voice.

## Findings

1. **Accuracy at turbo's cost.** On FLEURS the fine-tune is within 0.2 WER points of large-v3 (13.6 vs 13.4 %, CER better: 3.3 vs 3.5 %), with turbo's latency (p50 0.82 s vs 1.55 s) and memory (865 MB vs 2.13 GB). On the short sets it ties large-v3 on clean phrases and beats both models in noise (34.8 % vs 45.7 % and 50.0 %).
2. **Stray leading punctuation on 78 of 234 non-empty outputs** (54 of 100 FLEURS clips), e.g. ".A végeredmény egypontos győzelem volt" or "– Sok strand van". WER ignores it; a user would see it typed. It looks like a training-data habit (dialogue dashes, sentence splits) and is the easiest defect to remove (strip leading "." and dashes in cleanup).
3. **Empty output on clean one-word clips.** 7 of 132 short clips give no text, including clean "Holnap.", "Rablás." (twice) and "Liszt."; turbo and large-v3 give text for all of them. Not yet known whether the model emits nothing or only punctuation, or whether a noise guard in `vad.rs` (tuned on turbo's confidence) drops it.
4. **Skipped sentences in long dictations.** Long-set CER doubles (4.7 → 9.4 %). In `long-04` the whole NHK sentence is missing; in `long-06` the first half of a sentence ("Az Amerikai Egyesült Államokban … ismert, így") is gone, each time right where a stray "." appears. Silent loss of dictated text is the worst failure for this app.
5. **Invents text over music.** On the 15 s music bed it types "Hogyha nem tudom, hogy megfelelődjük." where both reference models type nothing.
6. **Numbers come out as words** ("negyven rab", "tizenkilenchez …") where turbo writes digits; the reference text mostly uses digits. Not a defect in itself, but a style change from the current models.

## What it means

A turbo-sized Hungarian model can reach large-v3's accuracy, which would halve the Hungarian release wait and memory. This fine-tune is not ready to replace large-v3: findings 3 and 4 lose dictated text. Worth one more round before deciding: strip leading punctuation, find what empties the one-word clips, and check whether the sentence skips come from the stray full stops (a segment ending early) or from the model. The choice of default model stays with the maintainers.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper && cd ..
# fine-tune: ggml-hu-whisper-large-v3-turbo-finetuned-q5_0.bin from
# huggingface.co/sarpba/whisper-hu-large-v3-turbo-finetuned-GGML (MIT); keep it outside the repo,
# with ggml-silero-v5.1.2.bin beside it
/usr/bin/time -l core/target/release/pusmogo-bench --model <model.bin> --json out.json \
  bench/audio/fleurs-hu/hu.tsv bench/audio/short/{hu,low,noisy}.tsv \
  bench/audio/synth/{silence,noise,noise-long}.tsv bench/audio/long/hu.tsv
```

The bench groups results by manifest file name, so FLEURS, short and long all land under `hu`; split them by clip id (`fleurs-`, `short-hu`, `long-`) to get the rows above.
