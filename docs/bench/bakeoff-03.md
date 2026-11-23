# bakeoff-03: can large-v3 ship without new noise guards?

Up: [../README.md](../README.md)
Verified: 2026-11-08

[bakeoff-02](bakeoff-02.md) showed large-v3 q5_0 cuts short Hungarian phrase errors by a third. The noise guards in `core/crates/pusmogo-core/src/vad.rs` (known phrases, confidence floors) were tuned on the shipped turbo q5_0. This report runs large-v3 over the silence, noise and noisy-short sets and times the streamed release wait on long dictations. Measurement only; no product code changed.

**Answer: no.** With the speech detector on, large-v3 writes text over music that turbo drops: a subtitle credit ("Feliratot készítette Amara.org közösségével") over 15 s of music and "NAMASTE" over 4 s. Without the detector it writes the credit over 6 of 8 noise clips. On long dictations it also makes the user wait about twice as long after the key is released (1.6 s vs 0.85 s mean) because its background passes fall behind real time.

## Noise and silence

Same manifests in the same order: `synth/silence` (6 clips), `synth/noise` (4 × 4 s: fan, typing, music, chatter), `synth/noise-long` (the same at 15 s), `short/noisy` (44 one-to-four word phrases over noise) and 20 spoken "Köszönjük…" phrases from [noise-05](noise-05.md) (the guard's "keep real thanks" check).

| model | detector | silence + noise clips with invented text (of 14) | noisy short WER % | "Köszönjük" WER % | finish p50 s |
|---|---|---|---|---|---|
| turbo q5_0 | on | **0** | 48.9 | 4.4 | 0.61 |
| turbo q5_0 | off | **0** | 48.9 | 4.4 | 0.61 |
| large-v3 q5_0 | on | 2 (music 4 s "NAMASTE", music 15 s Amara.org credit) | **40.2** | 8.9 | 0.76 |
| large-v3 q5_0 | off | 7 (Amara.org credit on typing, chatter, fan; "NAMASTE" on music) | **40.2** | 8.9 | 0.76 |

Silence (six clips, up to 5 s) stays empty with both models. The detector removes typing, fan and chatter before Whisper sees them, so the remaining leaks are music, which the detector hears as voice. "NAMASTE" is already a known phrase but large-v3 says it with enough confidence over music to pass the 0.85 floor; the Amara.org credit is not on the list at all. On the "Köszönjük" set large-v3 is worse over chatter ("2 szűnik szépen", "hogy előttetek"), while all 16 clean or fan/typing variants are right with both models.

## Release wait on long dictations

`pusmogo-bench --stream --release-after 0.3` on the 8-clip long set (526.6 s of Hungarian, 21–100 s each), as in [caption-01](caption-01.md). The user waits for any running background pass (release wait) and then `finish()`.

| model | previews | WER % | slowest background pass s (min–max per clip) | release waits | mean / max release wait s | finish mean / max s |
|---|---|---|---|---|---|---|
| turbo q5_0 | off | 19.7 | 0.77–0.90 | 1 of 8 | 0.05 / 0.38 | 0.85 / 0.96 |
| turbo q5_0 | on | 19.5 | 0.76–0.87 | 3 of 8 | 0.09 / 0.37 | 0.81 / 0.94 |
| large-v3 q5_0 | off | **14.9** | 1.31–2.62 | 3 of 8 | 0.19 / 0.93 | 1.59 / 2.03 |
| large-v3 q5_0 | on | 16.9 | 1.39–1.87 | 5 of 8 | 0.29 / 0.96 | 1.59 / 2.12 |

The app steps the session once per second of audio. Turbo's background passes take under 0.9 s, so they keep up with speech; large-v3's take 1.3–2.6 s, so passes queue behind each other and the release more often lands on one. The accuracy gain holds on long dictations (19.7 → 14.9 % WER).

Turning previews on changed the final text of 4 of 8 clips with turbo and 3 of 8 with large-v3 on this set ([caption-01](caption-01.md) found 0 changes on FLEURS without the 0.3 s release tail). [caption-02](caption-02.md) explains it: a preview delayed one chunk cut, and the later clips differ through carry-over between clips, not through preview state.

## What it means

Before large-v3 can ship, as an option or as the default:

1. Add the subtitle credits ("feliratot készítette amara.org közösségével", "feliratok az amara.org közösségétől") to the known phrases and re-check the music clips, where "NAMASTE" also passes the confidence floor.
2. Decide the step interval per model: with large-v3, step every 2 s (or skip a step while a pass runs) so background passes keep up, and re-time the release wait.
3. Re-run the "Köszönjük" set: two chatter clips got worse.

Short phrases and FLEURS gains stand ([bakeoff-02](bakeoff-02.md), [bakeoff-01](bakeoff-01.md)).

## Method note

`pusmogo-bench` loads the speech detector from the folder of the main model. Keep `ggml-silero-v5.1.2.bin` (hard link) next to any scratch model, or the "detector on" run is silently a "detector off" run: look for `loading VAD model` in stderr. The bakeoff-02 short-set run had no detector next to large-v3; rerun with it, all 132 outputs were identical, so its numbers stand.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
ln "$HOME/Library/Application Support/Pusmogo/models/ggml-silero-v5.1.2.bin" <scratch-dir>/
M=<scratch-dir>/ggml-large-v3-q5_0.bin   # sha256 in bakeoff-02
target/release/pusmogo-bench --model "$M" --json out.json ../bench/audio/synth/{silence,noise,noise-long}.tsv \
  ../bench/audio/short/noisy.tsv kj.tsv          # kj.tsv: the spoken "Köszönjük" clips, see noise-05
PUSMOGO_SPEECH_DETECTOR=0 target/release/pusmogo-bench --model "$M" ...   # detector off
target/release/pusmogo-bench --model "$M" --stream [--preview] --release-after 0.3 ../bench/audio/long/hu.tsv
```

The stderr lines `stream: … slowest step …` and `release wait behind a running pass: …` carry the streaming numbers. Apple M5 Pro with Metal; synthetic speech (macOS TTS, Tünde) is a proxy for a real voice.
