# testset-01: the recording kit's speaker folder through the bench

Up: [../README.md](../README.md)
Verified: 2026-11-09

Date: 2026-11-09 · Machine: Apple M5 Pro · Models: `ggml-large-v3-q5_0.bin`, `ggml-large-v3-turbo-q5_0.bin` · Audio: macOS TTS (Tünde for Hungarian, Eddy for English), whispered takes from `scripts/pseudo-whisper.py`. **Synthetic speech, not a real speaker.**

## Why

The recording kit ([bench/testset/README.md](../../bench/testset/README.md)) saves one speaker's takes as `<id>.wav` / `<id>-whisper.wav` with the reference beside each take. Until now no folder in that layout had gone through `scripts/testset-manifest.py` and `pusmogo-bench` end to end. This run is that check: it shows the chain works and gives a synthetic baseline to put next to the first real recordings.

## How

1. Render every script line with `say` + `afconvert -f WAVE -d LEI16@16000 -c 1` into a scratch folder as `<id>.wav` + `<id>.txt`: the format the app's `save_take` writes (16 kHz mono 16-bit).
2. Make the 20 `whisper=yes` lines into pseudo-whispered takes (`pseudo-whisper.py`, default settings, -44 dBFS) saved as `<id>-whisper.wav`.
3. Write `consent.txt` (`private …`), then `scripts/testset-manifest.py <folder> --by-category` → `normal 130/130, whisper 20/20, consent private`, plus seven per-category manifests.
4. `core/target/release/pusmogo-bench --model <model> <folder>/normal-{everyday,tech,names,numbers,hlr,vs,everyday-en}.tsv <folder>/whisper.tsv`, both models, exit 0.

Audio stayed under `/tmp` and was deleted afterwards.

## Results

| set | clips | large-v3 WER % | large-v3 CER % | turbo WER % | turbo CER % |
|---|---|---|---|---|---|
| everyday (hu) | 40 | 6.5 | 1.5 | 6.2 | 1.2 |
| tech jargon (hu) | 25 | 15.8 | 4.0 | 17.4 | 4.4 |
| names (hu) | 15 | 9.7 | 1.9 | 9.7 | 1.9 |
| numbers and dates (hu) | 15 | 14.3 | 5.9 | 15.4 | 6.9 |
| h/l/r-initial (hu) | 15 | 5.8 | 1.3 | 8.1 | 1.9 |
| "X vs Y" (hu) | 10 | 10.5 | 3.4 | 13.2 | 3.4 |
| everyday (en) | 10 | 1.2 | 0.2 | 0.0 | 0.0 |
| whispered (hu) | 20 | 23.5 | 12.5 | 23.5 | 12.0 |
| all | 150 | 11.2 | 3.9 | 11.8 | 3.9 |

Per-pass p50: large-v3 0.83 s, turbo 0.61 s (each take decoded whole, no streaming).

## Reading

- The chain works: a folder in the app's take layout becomes manifests and a per-category table with no hand editing. The `-whisper` takes score as their own set.
- Many Hungarian "errors" are TTS artefacts, not hearing: the voice blurs double consonants (*küldd → küld*, *állomáson → álomáson*, *tejjel → tejel*), so the everyday set under-reports what a real speaker would get right and the TTS run is no accuracy claim. Tech jargon (*pull request → pulregvestet*, *TypeScript → PestSript*) and numbers (Whisper writes digits for spoken numbers) are the weak categories, as on the earlier sets.
- Pseudo-whispered TTS (23.5 %) is far easier than pseudo-whispered real speech ([whisper-01](whisper-01.md): FLEURS 42.3 %). The whispered column only means something with real recordings.

## Next

Record one real speaker through Settings → Test recording and rerun step 3–4 on their folder; compare per category with this table.
