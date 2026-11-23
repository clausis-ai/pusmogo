# Bakeoff 01: which Whisper model for Hungarian (2026-11-08)

Chunk A3: compare the shipped model, `ggml-large-v3-turbo-q5_0.bin`, with a higher-precision turbo and with the full large-v3, on real Hungarian speech. Recommendation only; the default does not change in this commit.

**Setup.** Apple M5 Pro with Metal, idle machine, release `pusmogo-bench --features whisper` (engine + cleanup, warm engine, speech detector on). Models are the public ggml files from `ggerganov/whisper.cpp` on Hugging Face (MIT), kept outside the repo. Sets: the 100 FLEURS Hungarian dev clips of [fleurs-01](fleurs-01.md) (real read speech), and the synthetic English (20) and Hungarian-with-English-terms (30) sets of [synth-01](synth-01.md). Batch mode unless stated; `--stream --release-after 0.3` is the app's path with a realistic key release ([caption-01](caption-01.md)).

| model | download MB | peak memory MB | load s | FLEURS WER % | WER joined % | CER % | batch p50 / p95 s | stream release wait mean / max s | stream p50 s |
|---|---|---|---|---|---|---|---|---|---|
| turbo q5_0 (shipped) | 574 | 755 | 0.18 | 17.4 | 14.3 | 4.5 | 0.75 / 0.82 | 0.04 / 0.72 (11 clips) | 0.77 |
| turbo q8_0 | 874 | 1051 | 0.23 | 17.2 | 14.0 | 4.5 | 0.71 / 0.78 | not run | not run |
| large-v3 q5_0 | 1081 | 1553 | 0.30 | **14.1** | **11.3** | **3.6** | 1.13 / 1.40 | 0.11 / 1.11 (21 clips) | 1.17 |

Peak memory is the bench process (`/usr/bin/time -l`, peak footprint). The time a user waits after releasing the key is roughly stream p50 plus the release wait.

| model | synthetic English WER % (p50 s) | synthetic mixed HU/EN WER % (p50 s) |
|---|---|---|
| turbo q5_0 | 0.0 (0.70) | 18.7 (0.71) |
| large-v3 q5_0 | 0.0 (0.87) | 21.0 (0.91) |

Per clip on FLEURS against turbo q5_0:

| model | identical | better | worse | exact clips | clips with WER ≥ 40 % | empty |
|---|---|---|---|---|---|---|
| turbo q8_0 | 56 | 14 | 10 | 21 | 13 | 0 |
| large-v3 q5_0 | 19 | 42 | 12 | 20 | 5 | 0 |

## Findings

1. **turbo q8_0 is not worth it.** 0.2 WER points on FLEURS is noise (44 clips change, 14 better, 10 worse) for 300 MB more download and memory. Its slightly lower latency is within run-to-run drift.
2. **large-v3 q5_0 is clearly more accurate on real Hungarian:** WER 17.4 → 14.1 (19 % fewer word errors), CER 4.5 → 3.6, and the badly wrong clips drop from 13 to 5. The gains are on the hardest clips, where turbo garbles whole phrases: "A naponta kiosztott mindezt a 18 érem miatt…" → "A naponta kiosztott mindössze 18 érem miatt…" (exact), "Múlt tovább az ortodox…" → "Moldovában az ortodox…".
3. **It costs speed and memory.** Each pass is about 0.4 s slower (p50 1.17 vs 0.77 s after release), release waits behind a running chunk pass double (mean 0.11 s, up to 1.1 s), and peak memory doubles to 1.55 GB. The model takes 32 decoder layers instead of turbo's 4; the encoder is the same size.
4. **It is not better on everything.** On the synthetic mixed Hungarian/English set it scores worse (18.7 → 21.0; "pull requestet" became one word, "branchet" lost a letter), though it fixed "build" there. English is perfect for both on the synthetic set. The synthetic sets are TTS and easier than real speech.
5. **Parakeet v3 was not run.** It is not a whisper.cpp model, so trying it needs a second engine in the core. The license and engine-size cost would come first; out of scope for this report.

## Recommendation

Keep turbo q5_0 as the default: it keeps the after-release wait under 0.8 s and memory under 0.8 GB, which is what makes the app feel instant. Offer large-v3 q5_0 as an optional "most accurate" model for Hungarian users who accept about 0.4 s more wait and 1.5 GB of memory, if a model choice is wanted in Settings (the core engine loads any ggml Whisper file; the app would need a picker and a second download entry). Drop turbo q8_0 from consideration.

Reproduce: download the two extra models into a scratch folder, then for each model
`core/target/release/pusmogo-bench --model <file> --md out.md --json out.json bench/audio/fleurs-hu/hu.tsv`, and the same with `--stream --release-after 0.3`.
