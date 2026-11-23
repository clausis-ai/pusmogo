---
language: hu
license: mit
base_model: openai/whisper-large-v3-turbo
pipeline_tag: automatic-speech-recognition
tags:
  - whisper
  - whisper.cpp
  - ggml
  - hungarian
---

# Pusmogo HU 001

A Hungarian-only speech recognition model: `openai/whisper-large-v3-turbo` (MIT) trained further on Hungarian speech. It is the Hungarian model of Pusmogo, a free dictation app for macOS, and runs fully on the computer with [whisper.cpp](https://github.com/ggml-org/whisper.cpp).

**Use it for Hungarian only, with the language fixed to `hu`.** It was not trained on any other language; the app keeps the base model for English.

## File

| File | Format | Size | SHA-256 |
|---|---|---|---|
| `ggml-pusmogo-hu-q5_0.bin` | whisper.cpp, q5_0 | 574,041,195 bytes | `29a3c2a308a663f8d432d62225f41dfd6e10f79ed7856728eea18f7cdaf0b953` |

```bash
whisper-cli -m ggml-pusmogo-hu-q5_0.bin -l hu -f recording.wav
```

## Training data

All hours are after the audit (wrong or cut-off text, clipping, silence and over-represented speakers removed).

| Source | Hours | Licence |
|---|---|---|
| Common Voice Hungarian, Scripted Speech 27.0 | 86.0 | CC0-1.0 |
| VoxPopuli Hungarian, transcribed | 53.3 | CC0 (raw audio also under the European Parliament legal notice) |
| [FLEURS](https://huggingface.co/datasets/google/fleurs) Hungarian, train split only (Conneau et al., 2022) | 9.3 | CC-BY-4.0 |

25 % of the clips also have a pseudo-whispered copy (36.7 h), so whispered speech works better. No private, customer or app recordings and no synthetic voices were used for training. The training audio is not published here.

Training: full fine-tune, 2 epochs, batch 32, learning rate 1e-5 (linear, 500 warm-up steps), bf16, language token fixed to Hungarian; 98,987 clips (182 h), 2,047 clips (3.8 h) held out.

## Results

WER / CER in %, lower is better. Same engine, same settings, Apple M5 Pro. The FLEURS sentences are from the test split and were not in the training set. "Synthetic" sets use computer voices and are easier than real speech.

| Set | Clips | large-v3-turbo | large-v3 | Pusmogo HU |
|---|---|---|---|---|
| FLEURS Hungarian, real speech | 100 | 17.0 / 4.1 | 13.4 / 3.5 | **12.5 / 3.3** |
| FLEURS Hungarian, pseudo-whispered | 100 | 47.2 / 21.0 | 42.3 / 19.3 | **29.9 / 13.6** |
| Short phrases, synthetic | 44 | 29.3 / 7.9 | 18.5 / 4.9 | **12.0 / 3.4** |
| Short phrases, quiet, synthetic | 44 | 26.1 / 7.9 | **13.0 / 3.6** | 15.2 / 4.0 |
| Short phrases over noise, synthetic | 44 | 47.8 / 22.1 | 35.9 / 20.6 | **26.1 / 10.4** |
| Short phrases, pseudo-whispered, synthetic | 44 | 88.0 / 73.9 | 84.8 / 78.6 | **52.2 / 37.4** |
| Sentences, synthetic | 60 | 6.8 / 1.6 | 6.2 / 1.3 | **5.5 / 1.0** |
| Sentences over noise, synthetic | 20 | 6.9 / 1.3 | **2.8 / 0.3** | 5.6 / 2.1 |
| Hungarian with English terms, synthetic | 30 | **17.8 / 4.2** | 18.3 / 4.0 | 24.2 / 5.4 |

Median time for a FLEURS sentence: 0.29 s (large-v3-turbo 0.33 s, large-v3 0.73 s).

## Known weak spots

- Hungarian sentences with English terms ("deadline" can become "de ad line").
- Over music or long noise without speech it can write parliament openings ("Tisztelt képviselőtársaim!"), because VoxPopuli holds European Parliament speech. The app drops these phrases over noise; use a speech detector.
- Numbers, names and spelling are written as Whisper writes them; the app corrects part of this with fixed rules, not this model.

## Licence

Weights: MIT, like the base model. Data licences as in the table above. Made by [vatson.ai](https://vatson.ai).
