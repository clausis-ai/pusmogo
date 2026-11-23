# Earlier live caption from previews (caption-01)

Up: [../README.md](../README.md)
Verified: 2026-11-08

**Result: built in the core, off by default (`PUSMOGO_CAPTION_PREVIEW=1` turns it on in the app). Since 2026-11-08 previews use a short encoder window and cost no mean release wait (see the last section).** On the 100 FLEURS Hungarian clips the first caption appears 4.7 s into a dictation instead of 11.2 s on average, and now on every clip (before, 22 of 100 clips ended before a first chunk and showed no caption). The final text is unchanged (100 of 100 and 8 of 8 outputs identical). The cost is a key release that sometimes waits behind a running preview: mean wait 0.06 → 0.10 s on FLEURS, up to 0.79 s on the unlucky clips.

## How it works

Before this change the caption showed only text from finished streaming chunks ([stream-01](stream-01.md)), and the first chunk is cut 8 s or more in. Now, when `Session::step` has no chunk ready, `Session::preview` transcribes all the audio waiting after the last chunk, for the caption only:

- the first preview once 4 s are waiting (`PREVIEW_SECS`), then one per 3 s more audio (`PREVIEW_EVERY_SECS`);
- only while the last 0.5 s hold speech (`PREVIEW_VOICED_TAIL_SECS`): a silence at the end often means the key is about to be released;
- preview text is never part of the result: `finish()` transcribes that audio again, exactly as without previews.

On release the app calls `pusmogo_session_interrupt()`, which stops new previews and asks a running one to give up through whisper.cpp's abort callback. On Metal the abort is only checked between graph runs, so the probe below gave up 543 to 549 ms after the stop: in practice a release waits for a running preview to finish. That is why previews are off by default.

## Results

Turbo q5 (`ggml-large-v3-turbo-q5_0.bin`), Apple Silicon with Metal, warm engine. `pusmogo-bench --stream` feeds 100 ms pieces and steps once per second of audio, simulated in real time: a step is skipped while the previous pass would still be running, and a pass still running at the end of the clip is the release wait.

| set | previews | WER % | p50 s | p95 s | first caption (mean) | clips with a caption | release waits | mean / max wait s |
|---|---|---|---|---|---|---|---|---|
| FLEURS HU, 100 clips | off | 17.5 | 0.75 | 0.82 | 11.2 s | 78 | 17 | 0.06 / 0.78 |
| FLEURS HU, 100 clips | on | 17.5 | 0.78 | 0.84 | 4.7 s | 100 | 23 | 0.10 / 0.79 |
| long set, 8 clips (58–76 s) | off | 18.7 | 0.80 | 0.84 | 11.7 s | 8 | 2 | 0.09 / 0.62 |
| long set, 8 clips (58–76 s) | on | 18.7 | 0.80 | 0.84 | 4.7 s | 8 | 3 | 0.16 / 0.63 |

p50/p95 are `finish()` plus cleanup, not the release wait. Previews took 0.73 to 0.85 s each (one 1.39 s outlier), under the 1 s step.

**App check.** `PUSMOGO_CAPTION_PREVIEW=1 PUSMOGO_SELFTEST_WAV=bench/audio/long/long-07.wav`: the first "caption updated in 739 ms" arrived about 4.9 s into the clip with "Érdemes egy fél órát sétálgatni a megragadó kisfaluban", and a screenshot at about 6.5 s showed it above the sound bar. The final text matched the bench output.

## Reproduce

```bash
cd core && cargo build --release -p pusmogo-bench --features whisper
M="$HOME/Library/Application Support/Pusmogo/models/ggml-large-v3-turbo-q5_0.bin"
target/release/pusmogo-bench --model "$M" --stream ../bench/audio/fleurs-hu/hu.tsv
target/release/pusmogo-bench --model "$M" --stream --preview --abort-probe ../bench/audio/long/hu.tsv
```

The stderr lines `stream: … first caption … (chunks alone …)` and `release wait behind a running pass: …` carry the numbers above.

## Open

Turning previews on by default trades about 0.04 s of mean release wait (and an occasional 0.6–0.8 s one) for a caption 6 s earlier. That is a product call; it needs a real-voice test with the built-in mic.

## Tried: a cheaper release (2026-11-08, not shipped)

Goal: release wait back to the no-preview level (mean ≤ 0.06 s) with the first caption still ≤ 5 s. Same FLEURS set and model, `--stream`; outputs were identical to the baseline (0 of 100 changed) in every run.

| variant | previews | release waits | mean / max wait s | first caption (mean) |
|---|---|---|---|---|
| baseline (rerun) | on | 23 | 0.09 / 0.77 | 4.7 s |
| A: at most one preview between chunks | on | 21 | 0.09 / 0.82 | 4.8 s |
| B: cut a chunk only after speech resumes (or at 24 s) | on | 26 | 0.14 / 0.78 | 4.7 s |
| B | off | 20 | 0.12 / 0.81 | 11.6 s |

What the per-clip lines show:

- Most waits are behind a **chunk** pass, not a preview: the chunk is cut 10–14 s in, with 0.2–1.6 s of audio left before the release. Fewer previews (A) barely help.
- B made it worse. FLEURS clips end on the last word, so the bench releases while the speaker is still "speaking"; delaying the cut until speech resumes only moves chunk passes closer to the release.
- The bench releases at the last sample. A real user lets go some 0.2–0.5 s after the last word, which a chunk cut in that final pause would still hit. The bench cannot yet model that, so neither variant was shipped.

A chunk cut before the dictation reaches 29 s is redone by `finish()` whenever the whole dictation stays under 29 s, so for most dictations it only feeds the caption. With previews on, that caption work is duplicated; deferring the first chunk is the next thing to measure.

## A release that lags the last word (2026-11-08)

`pusmogo-bench --stream --release-after S` now appends S seconds of quiet room tone (about -54 dBFS, deterministic) to each streamed clip before the release, so the bench no longer releases on the last sample. Same FLEURS set and model, S = 0.3 s (the low end of the 0.2–0.5 s a real release lags):

| run | previews | release waits | mean / max wait s | first caption (mean) | WER % |
|---|---|---|---|---|---|
| release on the last sample (rerun) | on | 23 | 0.10 / 0.82 | 4.7 s | 17.5 |
| release 0.3 s after | on | 17 | 0.07 / 0.75 | 4.7 s | 17.4 |
| release 0.3 s after | off | 11 | 0.04 / 0.75 | 11.3 s (79 clips) | 17.4 |

- With the lag, previews on and off give the same output on all 100 clips. Against the run without the lag, 17 of 100 outputs change (the final pass now sees the trailing pause), with WER 17.5 → 17.4 %.
- With a realistic release, previews cost +0.03 s mean release wait (0.04 → 0.07 s) and leave the worst case unchanged (0.75 s both ways), in exchange for the first caption at 4.7 s instead of 11.3 s. This misses the earlier ≤ 0.06 s target by 0.01 s.
- Use `--release-after 0.3` for every later release-wait comparison; the no-lag numbers above overstate the wait.

## Tried: no chunk before 20 s with previews on (2026-11-08, not shipped)

Idea: with previews on, cut no chunk until 20 s of audio is waiting (instead of 8 s), since a chunk cut in a dictation that ends under 29 s is redone by `finish()` anyway, and each chunk pass is one more pass a release can wait behind. Same FLEURS set and model, `--stream --preview --release-after 0.3`, old and new bench binaries one after the other:

| run | chunks before release (all clips) | release waits | mean / max wait s | first caption mean / max | WER % |
|---|---|---|---|---|---|
| chunks from 8 s (current) | 84 | 17 | 0.064 / 0.71 | 4.71 / 5.7 s | 17.4 |
| chunks from 20 s | 1 | 26 | 0.105 / 0.78 | 4.73 / 7.7 s | 17.4 |

- Worse on every release-wait number. With no chunk to cut, the app makes a preview instead, every 3 s, and a preview covers all waiting audio, so the GPU is busy about as often as before and the release lands behind a preview more often. 1 of 100 outputs changed (one long clip, same WER).
- So the extra waits are not caused by chunks as such but by any pass near the release. Reverted; the remaining lever is when passes run (or how fast an interrupted pass stops), not which kind of pass runs.

## Shipped: faster previews with a short encoder window (2026-11-08)

An interrupted pass on Metal cannot stop early: whisper.cpp hands the abort callback only to the CPU backend, so the encoder graph runs to the end and the stop is seen only before decoding. A release that lands during a preview therefore waits for most of it. Instead of stopping previews faster, they now run faster: a preview sets whisper.cpp's `audio_ctx` to its audio length (`engine::preview_audio_ctx`: frames + 64, rounded up to 64, at least 768, full window near 30 s). Final passes keep the full 30 s window, so typed text is decoded exactly as before.

Two things had to go with it, both found on the bench:

- **A separate whisper state for previews.** With one shared state, a short-window pass changed what the next full-window passes decoded: 30 of 100 FLEURS outputs changed, WER 17.4 → 18.1 %, with identical chunk cuts. With a second state (made at the first preview), 0 of 100 changed. The second state costs about 245 MB of buffers (whisper.cpp's own figures: compute 202 MB, KV 42 MB), only while previews are on.
- **Cutting repeated text from a preview.** A short window sometimes makes Whisper say a sentence again past the end of the speech ([audioctx-01](audioctx-01.md)): 7 of 256 previews repeated a run of four words. `until_repeat` cuts the preview before the first repeated run: 0 of 256 after.

Same FLEURS set and model, `--stream --preview --release-after 0.3`, the same binary with `PUSMOGO_PREVIEW_FULL_WINDOW=1` for the old behaviour:

| set | preview window | release waits | mean / max wait s | first caption (mean) | outputs changed | WER % |
|---|---|---|---|---|---|---|
| FLEURS HU, 100 clips | full | 17 | 0.07 / 0.73 | 4.71 s | | 17.4 |
| FLEURS HU, 100 clips | short | 16 | 0.04 / 0.72 | 4.34 s | 0 of 100 | 17.4 |
| long set, 8 clips | full | 3 | 0.19 / 0.75 | 4.70 s | | 19.0 |
| long set, 8 clips | short | 3 | 0.10 / 0.38 | 4.33 s | 3 of 8 | 19.0 |

- Previews on now cost no mean release wait over previews off (0.04 s both on FLEURS). The worst case is still a chunk pass (0.72 s), which keeps the full window.
- On the long set 3 outputs changed with the same WER: faster previews free the engine earlier, so some chunks are cut at a different step (one clip shows a different tail length). On FLEURS chunk cuts and outputs were identical.
- Preview text length is about the same (1.73 vs 1.70 words per second of audio).
- App: `PUSMOGO_CAPTION_PREVIEW=1 PUSMOGO_SELFTEST_WAV=bench/audio/long/long-07.wav`: previews took 367 to 419 ms (chunk passes 715 to 994 ms), and the final text matched before.
