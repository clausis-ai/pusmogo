# whisper-04: training data for whispered Hungarian

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08 · Script: `scripts/make-finetune-data.py` · Smoke set: FLEURS Hungarian dev, 150 sentences · No model was trained.

## Why

[whisper-01](whisper-01.md) to [whisper-03](whisper-03.md) showed that whispered Hungarian fails inside the model: large-v3 goes from 13.4 to 42.3 % WER on whispered FLEURS and from 18.5 to 87.0 % on short phrases, and neither an audio chain nor any decoding setting helps. The model has to hear whispers in training. This step builds the training set; training itself waits for a go-ahead.

## What the script builds

`scripts/make-finetune-data.py <out-dir> --source <manifest>... [--real-whisper <manifest>...]` writes one set with three kinds of clip:

| condition | what it is |
|---|---|
| `normal` | the source clip as recorded |
| `pseudo_whisper` | a whispered copy (the [whisper-01](whisper-01.md) generator) of a share of the normal clips, 25 % by default (`--whisper-share`); the normal clip stays in the set, so each copy is paired with its original |
| `whisper` | real whispered recordings, for the recording kit's whispered pass; added as is, never augmented |

- **Deterministic.** The clips that get a copy and the noise in each copy come from a hash of the clip id and `--seed`. Two builds of the smoke set gave identical JSONL files and byte-identical whispered WAVs.
- **Eval split by sentence.** A sentence and its whispered copy always land on the same side (hash of the normalised reference, `--eval-share`, 5 % by default). With a speaker column in the manifest, `--holdout-speakers` moves whole speakers to eval instead, as the recording kit plan asks (two held-out speakers).
- **No bench leakage.** Any clip whose normalised reference matches a reference in `bench/audio/*/*.tsv` (or `--exclude`) is dropped and counted, and bench manifests are refused as sources unless `--allow-bench` is given.
- **No audio in git.** The script refuses an output folder inside the repository.
- **Output:** `audio/` (normal clips hard-linked, `pw-*.wav` copies), `train.jsonl` and `eval.jsonl` (`audio`, `text`, `language`, `condition`, `source`, `parent`, `speaker`), `stats.json` (clips and hours per split and condition, dropped clips, settings, licences given with `--licence`).

`scripts/fetch-fleurs.py --split train` now fetches the FLEURS training split (Hungarian: a 1.7 GB archive, CC-BY-4.0) in the same manifest format; `--every-recording` keeps every speaker's take of a sentence instead of the first. Only its 0.4 MB sentence list was fetched in this step (to count it), not the audio.

## Smoke run

FLEURS Hungarian dev has 150 distinct sentences; the first 100 are the bench's FLEURS set.

| check | result |
|---|---|
| bench manifest as a source | refused (exit 2) |
| output folder inside the repository | refused (exit 2) |
| bench-overlap guard | 100 of 150 clips dropped: exactly the bench's 100 |
| whispered copies | 15 of the 50 kept clips (30 %; 25 % expected, small sample) |
| split | train 43 normal + 12 whispered copies, eval 7 + 3; no sentence on both sides |
| real-whisper slot | 6 test clips with speakers `s1`/`s2`, `--holdout-speakers s2`: the 3 `s2` clips in eval, the 3 `s1` clips in train, the rest of the set unchanged |
| copies sound whispered | voiced frames 0–1 % (originals 56–66 %), level −43.9 dBFS (originals −24 to −28) |
| speed | 2.1 s wall for the 15 copies with 4 workers |

## Training plan (prepared, not run)


1. **Data.** FLEURS Hungarian train (3,095 recordings of 1,497 sentences, 9.3 h, CC-BY-4.0; `fetch-fleurs.py --split train --every-recording --count 2000`) plus Common Voice Hungarian (CC0; needs a manual download from the dataset site), each built with `--whisper-share 0.25`; real whispered pairs from the recording kit when they exist, two speakers held out.
2. **Model.** Start from `openai/whisper-large-v3-turbo` (MIT). Adapt the encoder (full or high-rank LoRA), keep the decoder mostly frozen: whispering changes the sound, not the language.
3. **Tooling.** Hugging Face `transformers` + `peft` on the Mac's GPU (MPS) reading the JSONL; convert the result with whisper.cpp's converter and quantise to q5_0 like the shipped models.
4. **Ship gate.** On the whispered sets from whisper-01, WER at least 50 % lower than the base model; on the normal short, FLEURS, long and noise sets, WER not worse (per-clip diff read by hand); no new inventions on the silence and noise sets.

Cost and time are not estimated here; nothing in this step spends money or trains a model.

## Reproduce

```bash
python3 -I scripts/fetch-fleurs.py --lang hu --split dev --count 160 --out /tmp/w3/fleurs-dev
python3 scripts/make-finetune-data.py /tmp/w3/set --source /tmp/w3/fleurs-dev/hu.tsv \
  --eval-share 0.1 --licence "FLEURS: CC-BY-4.0"
cat /tmp/w3/set/stats.json
```

Plain `python3`, not `-I`, for the second command: NumPy and SciPy are user-site packages on the test machine.
