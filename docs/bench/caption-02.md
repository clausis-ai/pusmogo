# Why caption previews changed the final text on the long set (caption-02)

Up: [../README.md](../README.md)
Verified: 2026-11-08

**Result: previews leave no Whisper state behind. On the long set, one chunk cut moved because its step ran after a preview, and the other changed clips differ only because Whisper state carries over from one clip to the next in the bench.** No code change is needed. [bakeoff-03](bakeoff-03.md) reported that turning previews on changed the final text of 4 of 8 long clips with turbo and `--release-after 0.3`. This note explains why.

## Runs

Turbo q5 (`ggml-large-v3-turbo-q5_0.bin`), speech detector on, `pusmogo-bench --stream --release-after 0.3`. Temporary changes made only for these runs, not committed: one env switch that let previews run without taking simulated time, and one that printed every chunk cut.

| run | set | outputs changed vs previews off | chunk cuts changed |
|---|---|---|---|
| previews off, repeated | long, 8 clips | 0 of 8 | 0 |
| previews on | long, 8 clips | 4 of 8 (long-04 to long-07) | 1 (long-04: 36.81 → 37.32 s) |
| previews on, no simulated time | long, 8 clips | 0 of 8 | 0 |
| previews on vs off, long-05 to long-07 run alone | 3 clips | 0 of 3 | 0 |
| previews on | FLEURS HU, 100 clips | 0 of 100 | 0 |

How these rows fit together:

- **Previews do not change Whisper state.** Previews run on their own Whisper state (`preview_state` in `engine.rs`). When they ran but did not move the schedule, all 8 outputs matched the no-preview run.
- **A preview can move a chunk cut.** The bench simulates real time, so a preview keeps the engine busy and the next `step()` runs later, with more audio. In long-04 that moved the cut at 36.81 s to 37.32 s, and the text near it changed ("atomerű működés" → "atomerőmű működéssől", clip WER 11.8 → 10.8 %). The app has the same timing, because its steps run on a wall clock too.
- **Later clips change only through carry-over.** long-05 to long-07 had identical cuts. Run on their own, previews changed none of them. Their outputs also change with the clips that run before them (2 of 3 differ between "alone" and "after long-00 to long-04", previews off). This is the known carry-over of Whisper state between calls, not a preview effect.
- **The 0.3 s release tail is not the cause.** On FLEURS with the tail, previews moved no cut and changed no output. WER was 16.7 % both ways.

## What it means for turning previews on

Previews change the final text only when they delay a chunk cut. A delayed cut is a different but equally valid cut point: one clip got slightly better, and none got worse. Nothing here argues against turning previews on. That decision still depends on the release wait ([caption-01](caption-01.md)).

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
M="$HOME/Library/Application Support/Pusmogo/models/ggml-large-v3-turbo-q5_0.bin"
target/release/pusmogo-bench --model "$M" --json off.json --stream --release-after 0.3 ../bench/audio/long/hu.tsv
target/release/pusmogo-bench --model "$M" --json on.json --stream --preview --release-after 0.3 ../bench/audio/long/hu.tsv
```

Compare the per-clip `output` fields only between runs that use the same manifest in the same order. Apple M5 Pro with Metal. The audio is synthetic speech (macOS TTS, Tünde), which stands in for a real voice.
