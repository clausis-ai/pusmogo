# stream-03: release wait in the app, large-v3 vs turbo

Date: 2026-11-08. Build: `./scripts/build.sh --whisper` at `7a7a32b`. Set: the 8-clip Hungarian long set (`bench/audio/long/hu.tsv`, 524 s, 58–76 s per clip). Apple M5 Pro with Metal, speech detector next to the models, previews off. Synthetic speech (macOS TTS) is a proxy for a real voice.

## Question

[stream-02](stream-02.md) measured the wait after release in the bench (large-v3, 2 s step: mean 1.76 s, worst 2.21 s). Does the app match it now that it loads large-v3 for Hungarian and steps every 2 s?

## Method

One app self-test per clip (`PUSMOGO_SELFTEST_WAV`, scratch profile, `-language hu`): the clip is fed to the streaming session in real time, as the microphone would, and "key up" comes after the clip's own length. The wait is the app's `total` timing (key up → text ready). Turbo runs pass `-model.hu 1` (the Settings choice "Fast"). Text is scored with `pusmogo-bench --hyp-file`.

## Result

| model | WER % | CER % | wait mean s | p50 s | worst s |
|---|---|---|---|---|---|
| large-v3 (2 s step) | **15.2** | **3.7** | 1.95 | 1.75 | 3.56 |
| turbo (1 s step) | 19.5 | 4.6 | **0.98** | **0.96** | **1.07** |

- Seven of eight large-v3 clips wait 1.57–1.90 s, in line with the bench. The eighth (long-00) waits 3.56 s, the same in three runs: a background pass started about 0.1 s before the release (~1.4 s), then the final pass covered an 18 s tail (~2.2 s). With the bench's timing the same clip did not hit a running pass.
- The app's mean (1.95 s) is 0.2 s above the bench's 1.76 s; the worst case is 1.35 s above it, from that one collision.
- Turbo waits about 1 s on every clip; large-v3 roughly doubles that and cuts word errors by about a fifth (19.5 → 15.2 %).

## What would help

The wait behind a pass that started just before the release is the avoidable part. Options: do not start a background pass when the last one finished less than a step ago and little new audio has arrived; or abort a running pass at release (`pusmogo-bench --stream --abort-probe` measures how fast an abort returns). Both need the per-clip output gate from stream-02, since chunk cuts move.

## Reproduce

```bash
for c in bench/audio/long/long-0{0..7}.wav; do
  PUSMOGO_PROFILE=/tmp/p-$(basename $c .wav).sqlite PUSMOGO_KEEP_AUDIO=0 PUSMOGO_SELFTEST_WAV=$PWD/$c PUSMOGO_SELFTEST_DELAY=6 \
    build/Pusmogo.app/Contents/MacOS/Pusmogo -language hu > /tmp/large-$(basename $c .wav).log 2>&1
done
# turbo: add -model.hu 1 after -language hu
grep -h 'timing: total' /tmp/large-long-0*.log
```
