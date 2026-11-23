# stream-02: step interval for large-v3 streaming

Date: 2026-11-08. Model: `ggml-large-v3-q5_0.bin` with the speech detector next to it. Set: the 8-clip Hungarian long set (`bench/audio/long/hu.tsv`, 526.6 s, 21–100 s per clip), streamed with `--release-after 0.3`, previews off. Apple M5 Pro with Metal; synthetic speech (macOS TTS) is a proxy for a real voice.

## Question

[bakeoff-03](bakeoff-03.md) found that large-v3's background passes (1.3–2.9 s) are slower than the once-a-second step the app uses, so a release often lands while a pass is still running and the user waits for it before `finish()` starts. Steps are already skipped while a pass runs (the app's `Core.feed` and the bench both do this). Does a slower step interval help?

## Result

`pusmogo-bench --stream --step-every S` (new flag; the default stays 1 s).

| step every | WER % | CER % | release waits | mean / max release wait s | finish mean / max s | wait + finish mean / max s | first caption, earliest–latest s |
|---|---|---|---|---|---|---|---|
| 1 s (today) | 14.9 | 3.8 | 3 of 8 | 0.21 / 1.00 | 1.70 / 2.16 | 1.92 / 3.16 | 10.1–18.5 |
| 1.5 s | 14.3 | 3.6 | 1 of 8 | 0.16 / 1.25 | 1.92 / 2.34 | 2.08 / 3.16 | not compared |
| **2 s** | 15.4 | 3.7 | 1 of 8 | **0.05 / 0.39** | 1.72 / 2.21 | **1.76 / 2.21** | 11.2–19.5 |

- At 2 s the time the user waits after the release (running pass plus `finish()`) drops from 1.92 to 1.76 s on average and from 3.16 to 2.21 s in the worst case.
- `finish()` itself does not get slower, although the audio left at release is longer on some clips (long-00: 0.6 → 8.9 s): the final pass is one 30 s encoder window whatever the tail length.
- WER moves within the run-to-run spread: all 8 outputs differ between 1 s and 2 s because chunks are cut at other pauses, and the three runs span 14.3–15.4 %. CER is 3.6–3.8 % in all three.
- The live caption from finished chunks appears about 1 s later (first caption +1.0 s on every clip).
- The 1.5 s run had slower passes throughout (slowest step 3.51 s vs 2.88 s), so its one release wait was long; it is not a better middle ground on this evidence.

## Decision for D1

When the loaded model is large-v3, the app steps the session every 2 s; turbo keeps 1 s (its passes take under 0.9 s, [bakeoff-03](bakeoff-03.md)). This goes in with the per-language model choice, since the app cannot load large-v3 yet.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
M="$HOME/Library/Application Support/Pusmogo/models/ggml-large-v3-q5_0.bin"
for s in 1 2 1.5; do
  target/release/pusmogo-bench --model "$M" --stream --release-after 0.3 --step-every $s \
    --json /tmp/step-$s.json ../bench/audio/long/hu.tsv 2> /tmp/step-$s.err
done
grep "release wait behind" /tmp/step-*.err
```

`finish` is the per-clip `latency_s` in the JSON; the release wait per clip is in the `stream:` lines on stderr.
