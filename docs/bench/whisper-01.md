# whisper-01: whispered Hungarian, baseline

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08 · Models: `ggml-large-v3-turbo-q5_0.bin`, `ggml-large-v3-q5_0.bin` · Binary: `pusmogo-bench` at 0bebc1e

## Why

A real whispered dictation ("Mi a lopás vs rablás?", 2.5 s, built-in microphone, -44 dBFS) came out as "Yellow Bash vs. Roblash" with large-v3 and "A yellow bashkával szállásával." with turbo; a Hungarian prompt and more gain did not help. Before trying fixes we need a whispered test set and a number to move. Nobody has recorded hundreds of whispered Hungarian clips yet (the recording kit, H5, will add real ones), so this round makes a synthetic one.

## Method

`scripts/pseudo-whisper.py` turns normal speech into pseudo-whispered speech with LPC analysis and resynthesis (details and sources in the script header): per 32 ms frame an order-18 all-pole vocal-tract filter is fitted to the pre-emphasised signal, its formants below 1.5 kHz move up by 150 Hz and its bandwidths widen, and it is driven by white noise instead of the voiced excitation (no pitch at all). Breath rumble below 300 Hz that follows the speech level, a -44 dBFS clip level and a -62 dBFS microphone noise floor are added.

**Check against the real clip** (`--features`, measured on the 300–3400 Hz band; the real recording was read in place, never copied):

| clip | level dBFS | voiced frames | tilt dB (0.1–1 kHz over 1–4 kHz) |
|---|---|---|---|
| TTS "Mi a lopás vs rablás?" (Tünde) | -15.0 | 74 % | 14.3 |
| same, pseudo-whispered | -43.9 | 2 % | 12.1 |
| real whispered dictation | -44.4 | 17 % | 12.3 |

Level and spectral tilt match; the real clip still has a little voicing (people half-whisper), the synthetic one has none, so it is the harder case. One more difference: the real clip swings ~20 dB between its quietest and loudest frames (p10 -61, p90 -41 dBFS), the synthetic one ~15 dB, which matters for the energy gate below.

**Sets:** the 44 short Hungarian phrases (`bench/audio/short/hu.tsv`, Tünde TTS) and the 100 FLEURS Hungarian clips (real read speech, CC-BY-4.0), each normal and pseudo-whispered (audio under `/tmp`, never committed). Default bench settings, speech detector on (as the app ships), then again with `PUSMOGO_SPEECH_DETECTOR=0`. Sequential runs, nothing else heavy running.

## Results

| model | set | normal WER / CER | whispered WER / CER | whispered, no output | whispered, dropped before Whisper | p50 s normal / whispered |
|---|---|---|---|---|---|---|
| turbo | short (44) | 31.5 / 9.6 % | **89.1 / 80.7 %** | 31 | 20 | 0.58 / 0.60 |
| turbo | FLEURS (100) | 17.0 / 4.1 % | **47.2 / 21.0 %** | 3 | 2 | 0.72 / 0.77 |
| large-v3 | short (44) | 18.5 / 4.9 % | **87.0 / 84.7 %** | 21 | 20 | 0.75 / 0.82 |
| large-v3 | FLEURS (100) | 13.4 / 3.5 % | **42.3 / 19.3 %** | 2 | 2 | 1.40 / 1.42 |

Speech detector off (whispered sets only):

| model | set | WER / CER | no output | dropped before Whisper |
|---|---|---|---|---|
| turbo | short | 88.0 / 76.2 % | 24 | 6 |
| turbo | FLEURS | 47.1 / 19.8 % | 2 | 0 |
| large-v3 | short | 85.9 / 87.3 % | 8 | 6 |
| large-v3 | FLEURS | 41.7 / 18.1 % | 0 | 0 |

"Dropped before Whisper" = the clip returned in under 0.1 s, so no decode ran.

The real whispered clip, for reference (detector on and off give the same text): turbo "A yellow bashkával szállásával.", large-v3 "Yellow Bash vs. Roblash" (WER 80 % each). Its pseudo-whispered TTS twin gives no text at all with either model, because the energy gate drops it.

## Findings

1. **Whispering roughly triples the error rate on sentences and wipes out short phrases.** FLEURS: large-v3 13.4 → 42.3 %, turbo 17.0 → 47.2 % WER. Short phrases: 18.5 → 87.0 % (large-v3), 31.5 → 89.1 % (turbo). Large-v3 stays ahead, but the gap closes on whispers.
2. **Our own gates drop short whispers before Whisper hears them.** On the short set 20 of 44 whispered clips never reach the model: the Silero speech detector drops 14, the frame-energy check (`vad::VoiceActivity`, 15 dB above the clip's floor) drops 6 more. Long whispered sentences pass (2 of 100). For a real user this means a whispered word types nothing, which is at least safe.
3. **When short whispers do reach Whisper, it invents.** With the detector off, large-v3 writes "NAMASTE" (Helló, Lámpa, Lift vagy lépcső?, Levelet írok, Hol a lámpa?) and "Köszönöm, hogy megnéztétek!" (Liszt, Hétfőn ráérsz?): the classic noise hallucinations, so to Whisper a short whisper looks like noise. Opening the gates alone would trade empty output for invented text; the hallucination guards already drop some of these.
4. **Sentences keep their shape.** Whispered FLEURS outputs are mostly the right sentence with misheard words ("zsiráf" → "zsirák", "végeredmény" → "végtelenül"), CER ~19 %. Context helps Whisper here; one- and two-word dictations have none.
5. **No speed cost.** p50 per pass is the same for whispered and normal input.

## Limits

Pseudo-whispers are a proxy: one TTS voice for the short set, no real half-whispering, a narrower level swing than the one real clip. Treat the short-set numbers as a worst case and the FLEURS numbers as the more realistic baseline. Real whispered recordings (H5, 20 lines per speaker) should replace this set as the gate.

## Next (W2, W3)

- W2: detect whispering (voicing ratio and tilt as above) and, only then, try gain normalisation, gentler gates, a larger beam, temperature fallback and the Hungarian prompt; gate every change on the normal sets too (per-clip diff), because opening the gates on noise brings back invented text (finding 3).
- W3: add pseudo-whisper augmentation (20–30 % of clips) to the Hungarian fine-tune data recipe; a whispered-speech fine-tune is the lever most likely to move finding 1.

## Reproduce

```bash
python3 scripts/pseudo-whisper.py bench/audio/short/hu.tsv /tmp/w1/short-w
python3 scripts/pseudo-whisper.py bench/audio/fleurs-hu/hu.tsv /tmp/w1/fleurs-w
(cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper)
core/target/release/pusmogo-bench --model <model> --json out.json /tmp/w1/short-w/hu.tsv
PUSMOGO_SPEECH_DETECTOR=0 core/target/release/pusmogo-bench --model <model> /tmp/w1/short-w/hu.tsv /tmp/w1/fleurs-w/hu.tsv
python3 scripts/pseudo-whisper.py --features <clip.wav>...
```

The script needs NumPy and SciPy (`python3 -m pip install --user numpy scipy`).
