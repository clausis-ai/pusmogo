# whisper-02: a whisper-mode audio chain, and detecting whispers

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08 · Models: `ggml-large-v3-q5_0.bin`, `ggml-large-v3-turbo-q5_0.bin` · Binary: `pusmogo-bench` at 76db621 · Sets and baseline: [whisper-01](whisper-01.md)

## Why

whisper-01 showed whispered Hungarian triples the error rate and that our gates drop short whispers. Two cheap in-app levers were tested first, offline, before writing any app code:

1. **A whisper-mode processing chain** applied to the audio before Whisper: band-limit 120 Hz–7.5 kHz, light spectral denoise, +5 dB around 2.5 kHz and +3 dB around 4 kHz, per-frame dynamic normalisation, loudness to -16 LUFS. On the one real whispered clip an earlier ad-hoc version of this moved turbo slightly closer.
2. **A whisper detector**: can whispered input be told apart from normal speech *and from noise* by simple measures, so that any whisper-only lever fires only on whispers?

## Method

The chain was run with ffmpeg (`highpass=f=120, lowpass=f=7500, afftdn=nr=10:nf=-60, equalizer=f=2500:g=5, equalizer=f=4000:g=3, dynaudnorm=f=150:g=15, loudnorm=I=-16:TP=-1.5:LRA=11`, output 16 kHz mono through ffmpeg's anti-aliased resampler) over the whispered *and* normal short (44) and FLEURS (100) sets, then through `pusmogo-bench` with the shipping defaults (speech detector on). Normal sets are included because a detector that misfires would apply the chain to normal speech. The real whispered clip was processed in a scratch folder, read in place and deleted afterwards.

Detector measures: `scripts/pseudo-whisper.py --features` (voiced-frame ratio, spectral tilt) and the 10th–90th percentile spread of 300–3400 Hz frame levels, over the normal short folder (176 clips incl. noisy and low-level variants), the whispered sets and the synthetic noise beds (`bench/audio/synth/noise*.tsv`, `silence.tsv`).

## Results: the chain

WER / CER %, detector on. "Gated" = no decode ran.

| model | set | baseline (whisper-01) | with chain | gated before → after |
|---|---|---|---|---|
| large-v3 | whispered short | 87.0 / 84.7 | 91.3 / 70.3 | 20 → 6 |
| large-v3 | whispered FLEURS | 42.3 / 19.3 | 43.1 / 18.2 | 2 → 0 |
| large-v3 | normal short | 18.5 / 4.9 | 15.2 / 4.2 | 0 → 0 |
| large-v3 | normal FLEURS | 13.4 / 3.5 | 14.1 / 3.7 | 0 → 0 |
| turbo | whispered short | 89.1 / 80.7 | 84.8 / 64.3 | 20 → 6 |
| turbo | whispered FLEURS | 47.2 / 21.0 | 53.6 / 24.7 | 2 → 0 |
| turbo | normal short | 31.5 / 9.6 | 31.5 / 8.9 | 0 → 0 |
| turbo | normal FLEURS | 17.0 / 4.1 | **25.1 / 7.8** | 0 → 0 |

Real whispered clip ("Mi a lopás vs rablás?"): large-v3 "Yellow Bash vs. Roblash" → "Yellow Batch vs. Roblox" (80 % WER both), turbo "A yellow bashkával szállásával." → "A yellow bash versus a strawblás." (closer in shape, WER 80 → 120 %).

## Results: detecting whispers

| clips | voiced ratio | tilt dB | level spread dB |
|---|---|---|---|
| normal short (176, incl. babble/fan/keys mixed in) | 0.24–1.00 (median 0.75) | 5.8–27.2 | 12.2–57.9 |
| normal FLEURS (100) | 0.37–0.82 | 0.7–24.0 | 32.3–51.9 |
| whispered short (44) | 0.00–0.12 | 9.6–16.6 | 9.0–19.8 |
| whispered FLEURS (100) | 0.00–0.04 | 5.6–16.2 | 18.1–20.7 |
| real whispered clip | 0.17 | 12.3 | 18.7 |
| fan / white noise / hum beds | 0.00 | -5.5–29.7 | 0.0–2.1 |
| babble / keyboard beds | 0.00 | -6.1 to -5.0 | 16.8–23.3 |
| music beds | 0.97 | 35 | 14.4 |

## Findings

1. **The chain is rejected.** It lets 14 more short whispers past the gates, but Whisper then invents on them ("Jó étvágyat kívánunk!" on 4 clips, "HALÁLOS!", "Köszönöm!" for "Igen."), so whispered short WER goes *up* with large-v3 (87.0 → 91.3 %) and whispered FLEURS does not move (42.3 → 43.1 %). A few clips do come out right only with it ("Haló, haló, haló, haló", "Hallod, amit mondok", "Köszönöm"), so there is some signal under the gates, but not enough to beat the invented text. On normal speech it is harmful for turbo (FLEURS 17.0 → 25.1 %); the large-v3 normal short gain (18.5 → 15.2 %) is 3 words on 44 clips, within run-to-run carry-over.
2. **Opening the gates for whispers alone does not help.** With the chain the gates pass almost everything, and the extra passes mostly produce stock phrases. This agrees with whisper-01 finding 3 (detector off: 85.9 % vs 87.0 %). The safe behaviour today, typing nothing for an unheard whisper, is better than typing an invented phrase.
3. **Whispers can be detected, but not by voicing alone.** Low voicing (< 0.2) also describes every noise bed. Adding spectral tilt > 3 dB (removes babble, keyboard, white noise) and level spread > 8 dB (removes fan and hum) separates all whispered clips from all normal clips and noise beds in these sets. The real clip's voicing (0.17) sits near the 0.2 line: people half-whisper, so a real detector needs real recordings (H5) to set its threshold.
4. **The model is the lever.** Neither the audio nor the gates are the main problem; Whisper's own decoding of unvoiced Hungarian is. This supports the W3 route (fine-tune data with whispered and pseudo-whispered audio) over further in-app signal processing.

## What is left of W2

Decoding levers on whispered input only (beam size, temperature fallback, no-speech threshold, the Hungarian prompt) remain to be measured, but whisper-01's detector-off run and finding 2 above bound them: anything that makes more short whispers decode also brings back stock phrases. A detector (finding 3) is worth building only together with a lever that wins; none has yet.

## Reproduce

```bash
# chain (any manifest; writes <out>/hu.tsv and WAVs)
for each clip: ffmpeg -i in.wav -af "<chain above>" -ar 16000 -ac 1 -c:a pcm_s16le out.wav
core/target/release/pusmogo-bench --model <model> --json out.json <out>/hu.tsv
python3 scripts/pseudo-whisper.py --features <clip.wav>...
```
