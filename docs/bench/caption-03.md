# caption-03: caption previews on by default, checked with large-v3

Date: 2026-11-08. Model: `ggml-large-v3-q5_0.bin` (the Hungarian default above 8 GB since D1), 2 s stream step, speech detector on. Apple M5 Pro with Metal. Synthetic speech (macOS TTS) is a proxy for a real voice.

## Question

[caption-01](caption-01.md) and [caption-02](caption-02.md) showed, with turbo, that caption previews bring the first caption forward (about 4.3 s instead of 11.3 s) with no change to the typed text and almost no change to the wait after release. Does that still hold with large-v3, whose passes are slower, before previews go on by default?

## Bench (`pusmogo-bench --stream --release-after 0.3 --step-every 2`)

| set | previews | WER % | typed text changed | release wait mean s | worst s | first caption |
|---|---|---|---|---|---|---|
| long (8 clips, 527 s) | off | 15.4 | | 0.01 | 0.10 | 11.0–19.3 s |
| long | on | 15.4 | 0 of 8 | 0.11 | 0.76 | 4.4–4.5 s |
| FLEURS HU (100 clips) | off | 13.7 | | 0.28 | 1.55 | none before release on 29 clips, else ~11.3 s |
| FLEURS HU | on | 13.7 | 0 of 100 | 0.33 | 2.38 | p50 4.5 s |

"Release wait" is the part of the wait spent behind a background pass still running at release; `finish()` itself is unchanged (long-set p50 1.33 → 1.35 s).

## App

One app self-test per long clip (`PUSMOGO_SELFTEST_WAV`, scratch profile, `-language hu`, clip fed in real time as in [stream-03](stream-03.md)), previews on (the new default) and off (`PUSMOGO_CAPTION_PREVIEW=0`). Wait = the app's `total` timing (key up → text ready); memory = `peak memory footprint` from `/usr/bin/time -l`; first caption = first `live text` line, measured from the end of the 6 s start delay.

| previews | WER % | CER % | wait mean s | p50 s | worst s | first caption | peak footprint |
|---|---|---|---|---|---|---|---|
| off | 15.2 | 3.7 | 1.76 | 1.53 | 3.26 | 11.5–19.6 s | ~2.0 GB |
| on | 15.2 | 3.7 | 1.71 | 1.66 | 2.02 | 4.8–4.9 s | ~2.35 GB |

- The typed text differs by one word on 2 of 8 clips ("mogyoró" → "magyaró", "...egyre" → "egyre"), the same WER: chunk cuts move a little with real-time timing.
- The wait is the same on average. The worst case is lower with previews on: long-00 (3.26 s off, the collision described in stream-03) did not hit a running pass this time. That collision is timing luck, not something previews fix; backlog item "release behind a running pass" stays.
- Previews cost about 350 MB of peak memory with large-v3 (turbo: ~245 MB, caption-01).
- Screenshots: the switch in Settings ([EN](../screens/settings-caption-switch-en.png)) and a preview caption about 6 s into a dictation, before the first chunk ([HU](../screens/caption-preview-hu.png)).

## Decision

Previews go on by default (decided 2026-11-08), with a Settings switch "Korai élő felirat" / "Early live caption" to turn them off. `PUSMOGO_CAPTION_PREVIEW=0|1` overrides the switch for tests.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
M="$HOME/Library/Application Support/Pusmogo/models/ggml-large-v3-q5_0.bin"
for set in long/hu.tsv fleurs-hu/hu.tsv; do
  target/release/pusmogo-bench --model "$M" --stream --release-after 0.3 --step-every 2 ../bench/audio/$set
  target/release/pusmogo-bench --model "$M" --stream --preview --release-after 0.3 --step-every 2 ../bench/audio/$set
done
# App: one self-test per long clip, as in stream-03.md; previews off with PUSMOGO_CAPTION_PREVIEW=0.
```
