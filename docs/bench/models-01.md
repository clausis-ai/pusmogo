# models-01: Canary 1B v2 next to Whisper large-v3 and turbo on Hungarian

Up: [../README.md](../README.md)
Verified: 2026-11-08

NVIDIA's `canary-1b-v2` (CC-BY-4.0, 25 European languages) is reported to match Whisper large-v3 on FLEURS Hungarian at a fraction of the time. This report measures it on our own sets, next to the two Whisper models the app ships today (large-v3 for Hungarian, turbo for English and 8 GB Macs). Measurement only: no app or core code changed, and no dependency was added.

**Answer: on accuracy it sits between turbo and large-v3. On speed it is in a different class.** FLEURS Hungarian 12.4 % WER (large-v3 13.4 %, turbo 17.0 %) at 0.13 s per clip instead of 1.39 s. A whole 60–76 s dictation, cut at pauses as the app does, takes 0.67 s in total (large-v3 needs ~1.6 s *after* release with streaming). Short Hungarian phrases are worse than large-v3 (31.2 vs 23.9 % WER on the 132 synthetic short clips) and better than turbo (35.5 %). Its two real costs: it takes no vocabulary prompt, so Dictionary and context hints only work as after-the-fact replacement, which recovers much less (name sets 30.2 / 19.1 % vs 13.2 / 0.0 % for large-v3 with hints), and it invents a short word on near-silence where Whisper's own guards type nothing. It does not help whispered speech.

## How it was run

- Model: `handy-computer/canary-1b-v2-gguf`, file `canary-1b-v2-Q8_0.gguf` (1.14 GB, Hugging Face commit `692af661`, SHA-256 `224f83d1…a5d289` checked against the repository's published hash), stored under `~/Library/Application Support/Pusmogo/` and never committed.
- Runtime: `transcribe.cpp` (MIT, ggml with Metal), commit `5700f12`, built in a scratch directory outside the repo; `transcribe-cli -l hu --batch … --batch-jsonl` with the language forced to Hungarian (the model requires a language hint and does not detect it). Default settings: greedy decoding, punctuation and casing on.
- Time per clip = the runtime's own mel + encode + decode milliseconds, after model load. A first run overlapped a Whisper bench on the GPU; every time below was taken on an idle GPU.
- Scoring: `pusmogo-bench --hyp-file`, so canary's text goes through the same `respell` + `clean()` and the same normaliser as the Whisper rows. Whisper rows: `pusmogo-bench --features whisper`, q5_0 models, current decoding and guards, same day (short, long and noise sets) or quoted from [whisper-01](whisper-01.md) (FLEURS and whispered sets, same binary).
- Unlike the Whisper rows, the canary rows have no speech detector, energy gate or hallucination guard in front of them: those live in our Whisper path. Integrated in the app, the detector and energy gate would run first; the Whisper-specific guards (no-speech probability, stock phrases) would not.
- Apple M5 Pro. Synthetic speech (macOS TTS) is a proxy for a real voice.

## Results

| set | clips | turbo WER % | large-v3 WER % | canary WER % | turbo p50 s | large-v3 p50 s | canary p50 s |
|---|---|---|---|---|---|---|---|
| FLEURS hu (real speech) | 100 | 17.0 | 13.4 | **12.4** | 0.72 | 1.39 | **0.13** |
| FLEURS hu, CER | 100 | 4.1 | **3.5** | 3.6 | | | |
| short hu | 44 | 31.5 | **18.5** | 21.7 | 0.72 | 0.80 | **0.03** |
| short low (lower, faster voice) | 44 | 25.0 | **13.0** | 30.4 | | | |
| short noisy (fan, babble, keys) | 44 | 50.0 | **40.2** | 41.3 | | | |
| short, hu + low + noisy | 132 | 35.5 | **23.9** | 31.2 | | | |
| short app (capture path) | 44 | 31.5 | **18.5** | 22.8 | | | |
| long hu, 58–76 s each, whole clip (batch) | 8 | 18.4 | **13.9** | 43.6 | 2.50 | 4.81 | 0.55 |
| long hu, cut at pauses into ≤ 28 s pieces | 8 | – | – | 14.6 | – | – | **0.67** total |
| name set, no hints | 6 | – | 26.4 | 34.0 | | | |
| name set, matching context | 6 | – | **13.2** (prompt) | 30.2 (respell only) | | | |
| inflected names, no hints | 8 | – | 27.7 | 31.9 | | | |
| inflected names, matching context | 8 | – | **0.0** (prompt) | 19.1 (respell only) | | | |
| whispered short (pseudo-whisper) | 44 | 89.1 | 87.0 | 90.2 | | | |
| whispered FLEURS (pseudo-whisper) | 100 | 47.2 | **42.3** | 48.7 | | | |
| silence + noise beds (clips with invented text) | 14 | 0 | 0 | 1 ("Köszönöm." on 1 s of silence) | | | |

Other checks:

| | large-v3 q5_0 | canary-1b-v2 Q8_0 |
|---|---|---|
| download | 1.08 GB | 1.14 GB (Q5_K_M would be 837 MB, not measured) |
| peak memory footprint | 2.06 GB (bench), 2.05 GB in the app | 1.36 GB with the weights read into memory; 0.21 GB plus the 1.14 GB file mapped read-only (the runtime's default) |
| model load (warm file cache) | 0.28 s | 0.06–0.3 s warm; 2.5 s on the first run after the build (Metal shader compile) |
| one-word clips with no output | 1 of 176 short clips | 1 of 176 |
| question marks kept on the 44 questions in the short sets | 32 | 26 |
| vocabulary prompt (Dictionary, context hints) | yes, up to 224 tokens | no; only a transcript prefix, which the runtime's authors found made the output loop when used as context |
| licence | MIT | CC-BY-4.0 (attribution in `NOTICE` if shipped); runtime MIT |
| runtime added to the app | none (whisper.cpp is in the 7.4 MB bundle) | the stripped all-models CLI is 8.6 MB; a canary-only build sharing whisper.cpp's ggml is unmeasured and would decide whether the 10 MB budget holds |

The real recordings in the private voice-debug folder were read in place, never copied (pieces cut for the long set were our own TTS clips). Fourteen of the seventeen files have exactly the long set's durations and are earlier self-test replays, so the only real voice there is the whispered clip ("mi a lopás vs rablás"): canary types "Yellow Bash vs. Rubens.", like large-v3's "Yellow Bash vs. Roblash". On the two sub-second real clips, where large-v3 typed nothing, canary typed "Hú!" and "Köszönöm.".

## Findings

1. **Speed is the prize, as reported.** 0.03 s for a short phrase and 0.13 s for a FLEURS sentence, against 0.80 s and 1.39 s for large-v3: 10–25 times faster. That is far under the CHUNKS B3 target (0.30 s), which Whisper cannot reach because its encoder always processes a 30 s window ([speed-01](speed-01.md)). It would also remove the 2 s background streaming step: a 76 s dictation cut at pauses costs 0.67 s in total, and the last piece, which is all a streaming session waits for after release, costs at most 0.29 s.
2. **Accuracy is close to large-v3 on sentences, behind it on short phrases.** FLEURS and the long set (cut at pauses) are within one point of large-v3 (12.4 vs 13.4 %, 14.6 vs 13.9 %). On short phrases it loses 7 points overall, mostly on the lower, faster voice (30.4 vs 13.0 %). Typical misses are single sounds in one word ("Rabblás", "Láncpa", "Héthúrá érsz"), and it drops question marks more often.
3. **Given a whole minute in one call, it skips text** (43.6 % WER, outputs 30–50 % shorter than the reference). Any integration must cut at pauses, as our streaming session already does for Whisper.
4. **No vocabulary prompt.** Dictionary entries and on-screen names could only be applied by rewriting the output (`context::respell` and dictionary replacement). On the name sets that recovers a third to a half of what large-v3 gains from the prompt. For names already on screen this is a visible regression.
5. **It invents short words on near-silence** ("Köszönöm." on 1 s of silence and on a real 0.6 s tap, "Hú!" on a 0.7 s one) where Whisper's guards type nothing. Our speech detector and energy gate run before any engine, but a canary path would need its own guard for very short input.
6. **Whispered Hungarian: no help** (90.2 / 48.7 % vs 87.0 / 42.3 % for large-v3).
7. **Memory is lower**: 1.36 GB with the weights loaded into memory vs 2.05 GB for large-v3, and only 0.21 GB of dirty memory with the default memory-mapped weights.

## Recommendation

Do not switch engines for v0.1. The speed gain is real and large, but the switch costs the Dictionary and context-hint quality the app is built around, short phrases (the most common dictation) get worse than with large-v3, and integration means a second ggml runtime whose size against the 10 MB budget is unknown. Worth revisiting as a "fast Hungarian" option (for example for 8 GB Macs, where turbo is the default today: canary is better than turbo on every Hungarian set here and uses less memory) once a canary-only build has been sized and a short-input guard exists.

Not measured: `parakeet-ultra` (post-training data undisclosed; its download would also take this run past its 6 GB budget) and the Core ML build of canary. A Whisper-compatible Hungarian fine-tune ([finetune-01](finetune-01.md), [finetune-02](finetune-02.md)) remains the route that keeps the prompt.
