# bakeoff-04: Parakeet v3 next to Whisper turbo

Up: [../README.md](../README.md)
Verified: 2026-11-08

CHUNKS A3 named NVIDIA's Parakeet TDT 0.6B v3 (CC-BY-4.0, 25 European languages including Hungarian) as a bakeoff candidate, and [bakeoff-01](bakeoff-01.md) left it untried because it is not a whisper.cpp model. It is the one untested way to reach the short-dictation speed target (CHUNKS B3, p50 ≤ 0.30 s), which turbo cannot reach by tuning ([speed-01](speed-01.md)). This report measures it on the same sets as turbo. Measurement only; no product code changed.

**Answer: it is roughly 10 to 15 times faster, but it cannot ship as the Hungarian engine.** A short pass takes 0.04 s instead of 0.60 s and FLEURS accuracy is the same, but Parakeet has no way to force the language. On one-word Hungarian clips it sometimes answers in Cyrillic (8 of 176 short clips), short-phrase accuracy is worse on every set, and its runtime alone is far over the 10 MB app budget.

## How it was run

- Model: `mlx-community/parakeet-tdt-0.6b-v3` (the NVIDIA weights converted for Apple's MLX, 2.5 GB float32 on disk, cast to bfloat16 at load), run with the public `parakeet-mlx` 0.7.0 Python package from a scratch virtual environment outside the repo. Weights live in `~/Library/Application Support/Pusmogo/hf-cache/` and are not committed.
- A scratch script reads each WAV directly (no ffmpeg in the timed path), warms the model up once, then times `get_logmel` + `generate` per clip and writes `id<TAB>seconds<TAB>text`.
- Scoring: the new `pusmogo-bench --hyp-file F` option runs that text through the app's `respell` + `clean()` and the bench's normalisation, so both engines are scored identically. Its latency column is the external engine's own time.
- Turbo: `pusmogo-bench --features whisper`, large-v3-turbo q5_0, current decoding, rerun on the same day; its short-set numbers match [bakeoff-02](bakeoff-02.md) exactly. Large-v3 columns are quoted from [bakeoff-01](bakeoff-01.md)–[bakeoff-03](bakeoff-03.md).
- Apple M5 Pro. Synthetic speech (macOS TTS) is a proxy for a real voice.

## Results

| set | clips | turbo q5_0 WER % | Parakeet v3 WER % | large-v3 q5_0 WER % | turbo p50 s | Parakeet p50 s | large-v3 p50 s |
|---|---|---|---|---|---|---|---|
| short hu | 44 | 31.5 | 35.9 | **18.5** | 0.60 | **0.04** | 0.73 |
| short low (lower, faster voice) | 44 | **25.0** | 38.0 | **13.0** | 0.60 | **0.04** | 0.73 |
| short noisy | 44 | 50.0 | 66.3 | **40.2** | 0.60 | **0.04** | 0.75 |
| short, hu + low + noisy | 132 | 35.5 | 46.7 | **23.9** | 0.60 | **0.04** | 0.73 |
| short app (capture path) | 44 | 31.5 | 39.1 | – | 0.60 | **0.04** | – |
| FLEURS hu | 100 | 17.0 | 16.8 | **14.1** | 0.74 | **0.11** | – |
| synth hu sentences | 60 | **6.8** | 8.6 | – | 0.64 | **0.06** | – |
| synth mixed HU/EN | 30 | **18.3** | 24.2 | – | 0.63 | **0.06** | – |
| synth en | 20 | **0.0** | 1.6 | – | 0.62 | **0.05** | – |
| silence + noise (clips with invented text) | 14 | 0 | 0 | 2 | – | – | – |

| | turbo q5_0 | Parakeet v3 (MLX) |
|---|---|---|
| model download | 574 MB | 2.5 GB as published (about 1.2 GB if stored as bfloat16) |
| peak memory footprint (FLEURS + short hu run) | 0.84 GB | 2.19 GB (1.3 GB in use after the run; the peak includes the float32 → bfloat16 cast at load) |
| runtime added to the app | whisper.cpp, already in the 4.7 MB bundle | MLX: `libmlx.dylib` 21.7 MB + `mlx.metallib` 214 MB (a Core ML or ONNX build would bring its own runtime) |
| language | forced per dictation (fn + ⇧) | detected by the model, cannot be forced |

## Findings

1. **Speed is in a different class.** 0.04 s per short pass and 0.11 s p50 on FLEURS sentences, against turbo's flat ~0.6 s encoder cost. It would meet B3 with a wide margin and remove the need for background passes on most dictations.
2. **The language cannot be chosen.** Parakeet v3 picks the language itself. On one-word clips it switched to Cyrillic 8 times out of 176 ("Róka" → "Рука", "Rizs" → "Ріж", "Rossz a hang" over key clicks → "Рус Оганг") and to English spellings elsewhere ("Szia" → "See all", "Rendben" → "Brand Ben", "Liszt" → "Least"). Pusmogo's v1 rule is that the user chooses the language and nothing is detected, so this alone rules it out as the Hungarian engine without a way to constrain its output.
3. **Short Hungarian phrases are worse, not better.** 46.7 % WER on the 132 short clips against turbo's 35.5 % and large-v3's 23.9 %. It does fix some of turbo's word-gluing errors, including the phrase from the first real-voice report ("Mi a lopás versus rablás?" exact, where turbo writes "versúsra blás"; "Hol a lámpa?" where turbo writes "Hololámpa"), but loses more on single words and in noise (66.3 % vs 50.0 %).
4. **Long-form Hungarian is on par.** FLEURS 16.8 % vs 17.0 % WER (CER 4.5 % vs 4.1 %); TTS sentences slightly worse (8.6 % vs 6.8 %); mixed Hungarian/English worse (24.2 % vs 18.3 %).
5. **No inventions on silence or noise.** All 14 silence and noise clips came out empty, matching turbo with the speech detector and better than large-v3.
6. **Integration cost is high.** A second runtime (MLX, Core ML or ONNX) and a second engine behind the `Engine` trait, plus about twice turbo's memory. The MLX runtime would have to be downloaded with the model, since it cannot fit the 10 MB bundle.

## Recommendation

Keep turbo q5_0 as the default for v0.1 and keep the 0.6 s short pass (the B3 option already recommended in [speed-01](speed-01.md)). Parakeet v3 is worth revisiting only if a build can restrict its output to one language, for example by masking non-Latin tokens during decoding, and only with a real-voice short-phrase set; on today's synthetic sets large-v3 remains the accuracy lever and Parakeet only the speed one.

## Reproduce

```bash
uv venv -p 3.12 /tmp/pk/.venv && uv pip install --python /tmp/pk/.venv/bin/python parakeet-mlx==0.7.0
export HF_HOME="$HOME/Library/Application Support/Pusmogo/hf-cache"
# a script that, per manifest clip, reads the 16 kHz WAV, runs get_logmel + model.generate
# on mlx-community/parakeet-tdt-0.6b-v3 and writes id<TAB>seconds<TAB>text to /tmp/pk.tsv
core/target/release/pusmogo-bench --hyp-file /tmp/pk.tsv bench/audio/short/{hu,low,noisy,app}.tsv
```
