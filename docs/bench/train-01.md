# train-01: the first Hungarian fine-tune of turbo

Up: [../README.md](../README.md)
Verified: 2026-11-23

Date: 2026-11-22/23 · Model: `openai/whisper-large-v3-turbo` (MIT), full fine-tune · Data: [data card](../data-card.md) · Machine for the bench: Apple M5 Pro, Metal, warm engine, speech detector on for every model.

## Answer

**The fine-tune beats large-v3 on real and whispered Hungarian at turbo's speed and size.** FLEURS Hungarian 13.4 → 12.5 % WER (large-v3 → fine-tune; turbo 17.0), short phrases 18.5 → 12.0 %, short phrases over noise 35.9 → 26.1 %, pseudo-whispered FLEURS 42.3 → 29.9 % and short phrases 84.8 → 52.2 %, at 0.29 s p50 on FLEURS (large-v3 0.73 s, turbo 0.33 s) in the same 574 MB q5_0 file as turbo. Two things got worse: Hungarian sentences with English terms (turbo 17.8 → 24.2 %, "deadline" → "de ad line", "Slacken" → "slakken") and quiet short phrases against large-v3 (13.0 → 15.2 %, still better than turbo's 26.1 %). Over music and long noise it wrote VoxPopuli's parliament openings ("Tisztelt képviselőtársaim!"); a known-phrase guard now drops them (see below). English is not a goal for this model (the app keeps base turbo for English); its numbers are listed for information.

## Data and training

| | |
|---|---|
| Sources (after `scripts/data-audit.py`) | Common Voice 27.0 hu 86.0 h (CC0), VoxPopuli hu 53.3 h (CC0), FLEURS train hu 9.3 h (CC-BY-4.0) |
| Pseudo-whispered copies | 25 % of the clips, 36.7 h (`scripts/pseudo-whisper.py` via `make-finetune-data.py`) |
| Train / eval | 98,987 clips, 182 h / 2,047 clips, 3.8 h; Hungarian only, language token fixed |
| Recipe | `scripts/train-whisper.py`: 2 epochs, batch 32, lr 1e-5 linear, warmup 500, bf16, gradient checkpointing, best checkpoint by eval WER |
| Machine | one A100 SXM4 80 GB, EU datacenter host, rented through `scripts/vast-train.py` with a 30 USD cap |
| Time and cost | 3 h 45 min training, 6,188 steps at 2.06 s; 4.56 USD for the run, about 0.35 USD for three smoke runs |
| Eval WER during training (400 eval clips) | 18.3 % at step 1,000 → 15.5 → 12.5 → 11.6 → 10.6 → 10.0 % at the end (CER 3.1 %) |
| Export | `scripts/export-ggml.sh` → q5_0, 574,041,195 bytes, the same size as the shipped turbo file |

The audit dropped 21.6 h of Common Voice through the per-speaker cap (one speaker had 15 % of all clips; cap 1,500 clips) and kept CV clips only with two or more up votes and no down vote.

## Results

`scripts/bench-finetune.sh` per model, sequential runs on an idle machine. Synthetic sets use macOS TTS voices and pseudo-whispered audio; they flatter real speech.

| set | clips | turbo WER / CER % | large-v3 WER / CER % | hu-run1 WER / CER % | turbo p50 | large-v3 p50 | hu-run1 p50 |
|---|---|---|---|---|---|---|---|
| fleurs-en: en/en | 100 | 5.9 / 2.6 | 4.8 / 2.1 | 6.7 / 2.8 | 0.25 s | 0.45 s | 0.23 s |
| fleurs-hu: hu/hu | 100 | 17.0 / 4.1 | 13.4 / 3.5 | 12.5 / 3.3 | 0.33 s | 0.73 s | 0.29 s |
| short: hu/hu | 44 | 29.3 / 7.9 | 18.5 / 4.9 | 12.0 / 3.4 | 0.18 s | 0.25 s | 0.17 s |
| short: low/hu | 44 | 26.1 / 7.9 | 13.0 / 3.6 | 15.2 / 4.0 | 0.17 s | 0.25 s | 0.17 s |
| short: noisy/hu | 44 | 47.8 / 22.1 | 35.9 / 20.6 | 26.1 / 10.4 | 0.18 s | 0.26 s | 0.17 s |
| synth: hu/hu | 60 | 6.8 / 1.6 | 6.2 / 1.3 | 5.5 / 1.0 | 0.22 s | 0.41 s | 0.21 s |
| synth: mixed/hu | 30 | 17.8 / 4.2 | 18.3 / 4.0 | 24.2 / 5.4 | 0.21 s | 0.37 s | 0.20 s |
| synth: en/en | 20 | 0.0 / 0.0 | 0.0 / 0.0 | 2.1 / 1.0 | 0.20 s | 0.31 s | 0.19 s |
| synth: silence/hu | 6 | 0.0 / 0.0 | 0.0 / 0.0 | 0.0 / 0.0 | 0.00 s | 0.00 s | 0.00 s |
| synth: noise/hu | 4 | 0.0 / 0.0 | 0.0 / 0.0 | 0.0 / 0.0 | 0.02 s | 0.02 s | 0.01 s |
| synth: noisy/hu | 20 | 6.9 / 1.3 | 2.8 / 0.3 | 5.6 / 2.1 | 0.24 s | 0.45 s | 0.21 s |
| whisper: fleurs/hu | 100 | 47.2 / 21.0 | 42.3 / 19.3 | 29.9 / 13.6 | 0.32 s | 0.71 s | 0.30 s |
| whisper: short/hu | 44 | 88.0 / 73.9 | 84.8 / 78.6 | 52.2 / 37.4 | 0.18 s | 0.25 s | 0.17 s |
## The parliament openings over noise

Without a guard the fine-tune wrote "Tisztelt képviselőtársaim!" over 4 s of music (2.25 s voiced, mean token probability 0.81) and "Elnök Úr, tisztelt Képviselőtársaim!" over 15 s of noise (8.4 s voiced, 0.79). 395 of the 18,131 VoxPopuli training clips contain "Elnök úr". The same phrases spoken (TTS) decode at 0.95. Both are now known phrases in `vad.rs`, so the existing rule (a known phrase over more than 2 s of voiced audio needs 0.90) drops them over noise and keeps them spoken: every noise and silence clip, short and 15 s, is empty again.

## Not adopted: Latin-only decoding

`PUSMOGO_HU_ALPHABET=1` blocks every token that starts a non-Latin character. Base turbo wrote no such character on 288 Hungarian clips, normal and whispered, and the filter still moved beam scores (WER +0.3 to +0.6 points), so it stays off.

## Next

1. A live test with a real voice (`bench/compare/phrases.tsv`, 30 phrases, plus whispered takes): the realistic number the 3-6 % WER goal is about.
2. Run 2: Hungarian sentences with English terms, noise-only clips with an empty label, VoxPopuli greetings thinned, Granary filtered by two-engine agreement.
3. A Hungarian model slot in the app (today only turbo and large-v3 exist), then the Hugging Face release with a model card per the data card.
