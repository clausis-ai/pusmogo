# speed-01: where a short dictation's time goes

Up: [../README.md](../README.md)
Verified: 2026-11-08

A short Hungarian dictation takes about 0.6 s per engine pass with large-v3-turbo q5_0 on an Apple M5 Pro, against a target of 0.30 s (CHUNKS B3). This report splits that time into its parts and measures the decoding knobs that could plausibly lower it.

## Result

**Nothing ships.** The audio encoder takes about 96 % of a short pass, and its cost does not depend on how long the clip is: whisper.cpp always encodes a 30 s window. Beam width, thread count and weight quantisation do not move it. With this model, the 0.30 s target cannot be reached by tuning the decoder.

## Time split per pass

Measured inside the engine with a temporary probe: `pcm_to_mel` and `encode` were timed on their own before each normal `full()` call. Averages per pass, same binary, one run after the other:

| setting | passes on clips < 5 s | mel ms | encoder ms | full pass ms | decoder and rest ms |
|---|---|---|---|---|---|
| beam 5 (current) | 133 | 0.7 | 575 | 597 | ~22 |
| beam 3 | 133 | 0.7 | 598 | 618 | ~20 |
| greedy | 133 | | 620 | 632 | ~12 |
| q8_0 weights, beam 5 | 133 | | 578 | 597 | ~19 |

On FLEURS (5–30 s clips) the decoder share grows to about 130–140 ms per pass with beam 5, on top of the same ~575–610 ms encoder. The run-to-run drift between rows (±25 ms on the encoder) is larger than any difference the decoding setting makes; the machine warmed up over the three back-to-back runs. Earlier measurements already showed that the CPU thread count makes no difference, because the encoder runs on the GPU ([flashattn-01](flashattn-01.md)).

## Accuracy of the same runs

Short phrases (`short-01` sets: 44 clean, 44 quiet, 44 noisy) and 100 FLEURS Hungarian clips, 232 clips in all. "hu" below is the 44 clean short phrases plus FLEURS. Latency columns are left out: the probe ran the encoder twice per pass.

| setting | hu WER % | low WER % | noisy WER % | all WER % | all CER % |
|---|---|---|---|---|---|
| beam 5 (current) | 18.0 | 25.0 | 50.0 | 19.7 | 5.3 |
| beam 3 | 18.4 | 26.1 | 51.1 | 20.2 | 5.4 |
| greedy | 18.1 | 28.3 | 53.3 | 20.1 | 5.4 |
| q8_0 weights, beam 5 | 17.5 | 23.9 | 51.1 | 19.2 | 5.1 |

Per-clip comparison against beam 5: beam 3 made 13 clips better and 17 worse, greedy 17 better and 21 worse, q8_0 24 better and 17 worse (and 52 changed with the same score). Beam 5 stays the right default: narrower beams lose accuracy and save no time.

## Findings

1. **The encoder is the cost.** 575–620 ms of a ~600 ms pass, for 0.4 s clips and 30 s clips alike. Decoding a short phrase takes about 20 ms even with 5 beams.
2. **Quantisation is not a speed lever on Metal.** q8_0 weights (874 MB against 574 MB for q5_0) encode at the same speed. They score 0.5 WER points better on these sets, within the noise of one run; not worth 300 MB more download on this evidence alone.
3. **What could reach 0.30 s**, each a separate decision with a cost:
   - A shorter encoder window: about 2.2× faster but +9 WER points on real speech ([audioctx-01](audioctx-01.md)). Rejected.
   - A smaller or different model (turbo is already the fast large model; small or medium lose much more Hungarian accuracy; a non-Whisper engine such as Parakeet is untried).
   - Hiding the wait rather than cutting it: the release wait on longer dictations is already ~0.05 s because passes run while the user speaks ([stream-01](stream-01.md)); for a single short phrase the one pass after release is the whole wait.

## Reproduce

Add a timing probe around `self.state.full(params, samples)` in `WhisperEngine::run` (`core/crates/pusmogo-core/src/engine.rs`): time `self.state.pcm_to_mel(samples, threads)` and `self.state.encode(0, threads)` before it and print both with the `full()` time. Then:

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper && cd ..
for b in 5 3 1; do PUSMOGO_BEAM=$b core/target/release/pusmogo-bench --json /tmp/speed-b$b.json \
  bench/audio/short/{hu,low,noisy}.tsv bench/audio/fleurs-hu/hu.tsv 2>/tmp/speed-b$b.err; done
core/target/release/pusmogo-bench --model ggml-large-v3-turbo-q8_0.bin --json /tmp/speed-q8.json \
  bench/audio/short/{hu,low,noisy}.tsv bench/audio/fleurs-hu/hu.tsv
```

The q8_0 file is the public `ggml-large-v3-turbo-q8_0.bin` from the whisper.cpp model repository on Hugging Face.
