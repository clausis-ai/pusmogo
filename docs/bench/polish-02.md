# polish-02: two tiny local language models as a polish pass (part 2a)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08. Question: can an untrained, very small local language model, prompted to edit only, fix what [polish-01](polish-01.md) found left after Whisper and the rule cleanup (self-corrections, missing commas) without changing meaning, within the pass bar (quality up, no meaning change on `safety`, p50 added latency ≤ 0.3 s, memory ≤ 0.5 GB)?

**Answer for these two models: no.** Neither improves real speech, and the one that helps a little on the hand-written set needs more than 0.5 GB on long inputs. The two candidates left from the plan are a punctuation tagger and EuroLLM-1.7B (part 2b).

## Candidates

| model | licence (Hugging Face card, 2026-11-08) | Hungarian | file | size |
|---|---|---|---|---:|
| Qwen3-0.6B | Apache-2.0 | listed among its 119 languages | `unsloth/Qwen3-0.6B-GGUF`, Q8_0 | 640 MB |
| Gemma 3 270M (instruction tuned) | Gemma Terms of Use (custom; redistribution allowed with its use restrictions passed on, not an OSI licence) | multilingual pre-training | `unsloth/gemma-3-270m-it-GGUF`, Q8_0 | 292 MB |

Not downloaded: EuroLLM-1.7B-Instruct (Apache-2.0, Hungarian is one of its listed languages). Its Q4_K_M file alone is about 1.1 GB, so it cannot meet the 0.5 GB memory bar; part 2b measures it only if the quality case justifies relaxing that bar.

## Setup

- llama.cpp 0.9.0 (Homebrew), Metal, `llama-server -c 2048 --jinja` with n-gram speculative decoding over the prompt (`--spec-type ngram-simple --spec-ngram-simple-size-n 3 --spec-ngram-simple-size-m 24`), so copied stretches of the input decode in batches. Temperature 0, thinking off for Qwen3.
- `scripts/polish-llm.py`: Hungarian system prompt (punctuation and casing only; keep only the corrected part of a self-correction; never add, drop or replace another word; numbers and spelling unchanged; return the text as is when it is already right) plus four worked examples that share no item with the set. Input is the rule-cleaned text (`clean_text`). Writes one line per item and the per-item wall time.
- Scored with `scripts/score-polish.py --guard` on the text set, the TTS outputs of the same 40 items through large-v3, and 240 stored large-v3 outputs of real speech (FLEURS HU 152, short low-volume 44, short noisy 44).
- Memory: `footprint` of the `llama-server` process, `phys_footprint_peak` after all runs.

## Results

Text set (40 items, input is engine-style text), after the safety check:

| system | subset | WER | exact | F1 | F1(,) | fallbacks |
|---|---|---:|---:|---:|---:|---:|
| rules (today) | all | 9.6 % | 10 | 0.69 | 0.26 | |
| Qwen3-0.6B | all | 8.8 % | 14 | 0.73 | 0.38 | 10 |
| Qwen3-0.6B | selfcorr | 52.3 % | 1 | 0.58 | 0.11 | |
| Qwen3-0.6B | commas | 0.0 % | 1 | 0.65 | 0.18 | |
| Qwen3-0.6B | safety | 0.0 % | 10 | 1.00 | 1.00 | |
| Gemma 3 270M | all | 9.6 % | 10 | 0.69 | 0.26 | 15 |

Real speech and TTS (large-v3 outputs; the input of the polish pass is what Pusmogo types today):

| set | system | WER | exact | F1 | F1(,) | fallbacks |
|---|---|---:|---:|---:|---:|---:|
| real speech, 240 | rules | 15.6 % | 69 | 0.72 | 0.77 | |
| real speech, 240 | Qwen3-0.6B | 15.6 % | 74 | 0.71 | 0.77 | 32 |
| real speech, 240 | Gemma 3 270M | 15.6 % | 72 | 0.72 | 0.77 | 88 |
| FLEURS HU, 152 | rules | 14.6 % | 33 | 0.76 | 0.79 | |
| FLEURS HU, 152 | Qwen3-0.6B | 14.6 % | 37 | 0.76 | 0.79 | |
| TTS, 40 | rules | 16.1 % | 21 | 0.81 | 0.70 | |
| TTS, 40 | Qwen3-0.6B | 15.3 % | 22 | 0.83 | 0.73 | 7 |
| TTS, 40 | Gemma 3 270M | 16.1 % | 21 | 0.81 | 0.70 | 13 |

Cost (added per dictation, Apple Silicon, Metal):

| model | p50 | p95 | max | peak memory |
|---|---:|---:|---:|---:|
| Qwen3-0.6B Q8_0 | 0.04 s | 0.13–0.14 s | 0.36 s | 377 MB on the short set, 1014 MB after the long real-speech inputs |
| Gemma 3 270M Q8_0 | 0.03 s | 0.05–0.08 s | 0.13 s | 247 MB |

## Findings

1. **Speed is not the problem.** With prompt-copy speculative decoding both models add 0.03–0.04 s at p50, far under the 0.3 s bar.
2. **Quality is.** Qwen3-0.6B fixes 1 of 10 self-corrections and raises comma F1 from 0.26 to 0.38 on the text set, but on real speech it changes nothing measurable (F1 0.72 → 0.71, comma F1 0.77 both) and 32 of 240 answers change words and fall back. Typical misses: it keeps both halves of a correction, or drops the cue alone ("háromkor, négykor"); it drops ordinary words ("Mivel esik az eső" → "Az eső"). Gemma 3 270M returns its input or copies a worked example (`sc02`, `sc03` came back as the shopping-list example); nothing it returns improves a score.
3. **Memory**: Qwen3-0.6B's footprint grew to about 1 GB over long inputs (compute buffers sized by the longest prompt), over the 0.5 GB bar; a smaller context and batch might hold it down, not measured.
4. **The safety check had two holes, now closed** (`score-polish.py`). Qwen3 dropped "vagy nem" from the end of the safety item "Szerinted ez jó ötlet vagy nem?" and the check allowed it, because the dropped run ended in the cue "nem". It also allowed dropping the cue alone. The check now also requires a word before the cue inside the dropped run and a kept word after it. References still pass 40/40; on the Qwen3 text-set run the safety subset is 10/10 unchanged after the fix (9/10 before).

## Next

- Part 2b: a Hungarian punctuation/casing tagger (token classification; commas are its whole job, it cannot rewrite words) and, only if worth the memory, EuroLLM-1.7B.
- Self-corrections may be better served by rules than by a tiny model: the cue list and the guard's own shape ("<wrong part>, <cue>, <correction>" of the same kind) already describe them; a rule pass that proposes the drop and the guard that checks it is worth a measured try.
- Both downloaded models were deleted after the run (budget).
