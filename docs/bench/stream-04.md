# stream-04: a key release abandons a chunk pass the final pass would redo

Date: 2026-11-08. Build: `./scripts/build.sh --whisper`, base `d1f7d12` vs the change. Set: the 8-clip Hungarian long set (`bench/audio/long/hu.tsv`, 524 s, 58–76 s per clip), large-v3 q5_0, 2 s step, Apple M5 Pro with Metal, default settings (early live captions on). Synthetic speech (macOS TTS) is a proxy for a real voice.

## Question

In [stream-03](stream-03.md) one clip waited 3.56 s after the release because a chunk pass had started just before it. That pass was wasted work: `Session::finish` redoes every chunk that starts within 29 s of the end, together with the tail, and a chunk is cut once 8–24 s are waiting, so a chunk pass running at the release is almost always redone. Can the release abandon it instead of waiting for it?

## Change

- `Session::step_until` (used by the C ABI step): when the chunk starts within `FINAL_PASS_SECS` of the end of the audio, it runs through `Engine::transcribe_abortable` with the release flag that `pusmogo_session_interrupt` already sets. Same decoding and state as a normal pass; it only gives up when the flag is set. A chunk further back always runs to the end.
- `Session::finish` cuts chunks first while more than one final pass of audio is waiting, so every engine call stays one window even if a release abandoned a chunk and more audio followed. `transcribe_all` now relies on it.

## How fast an abort returns

`pusmogo-bench --stream --abort-probe` now also stops a 20 s full-window pass 100 ms in: on three long clips the pass takes 1154–1396 ms whole and gives up 566–568 ms after the stop. On Metal the abort is checked between graph runs, so a release during the encoder still waits for the encoder (~0.65 s); a release during decoding returns almost at once.

## Result (in-app release wait, `timing: total`)

| run | long-00 | 01 | 02 | 03 | 04 | 05 | 06 | 07 | mean s | p50 s | worst s | WER % |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 2.34 | 1.98 | 1.99 | 1.54 | 1.77 | 2.11 | 1.84 | 2.62 | 2.02 | 1.98 | 2.62 | 15.2 |
| change, run 1 | 2.06 | 1.66 | 1.69 | 1.50 | 1.73 | 1.97 | 1.52 | 1.50 | 1.70 | 1.67 | 2.06 | 15.2 |
| change, run 2 | 2.06 | 1.60 | 1.69 | 1.43 | 1.64 | 1.91 | 1.41 | 1.40 | **1.64** | **1.62** | **2.06** | 15.2 |

- The mean wait drops by about a third of a second and the worst case by 0.56 s; no clip is slower.
- Text: identical on 7 of 8 clips; long-06 differs by one word in run 1 only ("ráállással" vs "ráládással", reference "rálátással"), and run 2 matches the base exactly, so it is run-to-run variation, not the change.
- The bench (`--stream`) still calls the plain `step`, so its numbers do not move; on the long set the tail before `finish` is at most 21.1 s, so the new loop in `finish` never runs there. FLEURS clips are under 29 s, so neither change reaches them.

## Reproduce

```bash
# one self-test per clip, scratch profile; APP= points at another build for the A/B
for c in bench/audio/long/long-0{0..7}.wav; do
  n=$(basename $c .wav)
  PUSMOGO_PROFILE=/tmp/s04/p-$n.sqlite PUSMOGO_KEEP_AUDIO=0 PUSMOGO_SELFTEST_WAV=$PWD/$c PUSMOGO_SELFTEST_DELAY=6 \
    ${APP:-build/Pusmogo.app}/Contents/MacOS/Pusmogo -language hu > /tmp/s04/new-$n.log 2>&1
done
grep -h 'timing: total' /tmp/s04/new-long-0*.log
```
