# polish-07: a small language model picking among Whisper's own candidates (round 2, part 2b)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08. Question: [polish-06](polish-06.md) showed that a text-only pass cannot tell a fix from a new error of the same shape. If a language model may only *pick* among transcripts Whisper itself produced for the audio, it can never type a word Whisper did not hear. Does that lower Hungarian WER, and at what cost?

**Answer: not enough to ship.** Whisper's alternatives do hold better transcripts (the best candidate per clip would take FLEURS HU from 13.4 % to 11.4 % WER), but Qwen3-1.7B cannot find them. Its best setting gains 0.5 points on FLEURS and nothing on short phrases, and that setting was tuned on the same clips. Producing the candidates costs two extra full decodes, about 1.7 s more wait on a sentence, more than three times the 0.5 s limit.

## Setup

- Speech: large-v3 q5_0, the Hungarian default, `pusmogo-bench` on FLEURS HU (100 clips) and `short-01` (clean, low-volume and noisy, 132 clips), Silero detector on, no prompt. Apple M5 Pro, Metal.
- Candidates: six runs over the same manifests in the same order, each with a fresh model and state. The shipped decoding (beam 5 under 20 s), greedy (`PUSMOGO_BEAM=1`), and sampling at temperature 0.3, 0.5, 0.7 and 1.0 with no temperature fallback. Sampling used a temporary environment switch in `engine.rs` (greedy strategy, `set_temperature(t)`, `set_temperature_inc(0.0)`), reverted after the runs. whisper.cpp keeps its beam alternatives internal, so separate decodes are the way to get them without patching it.
- Picker: `scripts/nbest-rescore.py`. For each clip the distinct outputs are the candidates. Each is scored by its summed log-probability under Qwen3-1.7B Q4_K_M (the hash-checked file from [polish-05](polish-05.md), llama-cpp-python 0.3.36 with Metal in a scratch venv) after a fixed Hungarian prefix. Two options: `--vote W` adds W per run that produced the candidate (agreement), and `--len-guard F` drops candidates whose word count differs from the beam output's by more than F.
- Scoring: the bench's own per-clip word errors, so every row uses the same normaliser.

## Results

| system | FLEURS WER | FLEURS exact | short WER | short exact |
|---|---:|---:|---:|---:|
| beam 5 (shipped) | **13.4 %** | 23 | **25.7 %** | 88 |
| greedy | 14.1 % | 20 | 26.1 % | 84 |
| sampled, T 0.3 | 13.9 % | 22 | 27.2 % | 82 |
| sampled, T 0.5 | 14.9 % | 22 | 29.7 % | 79 |
| sampled, T 0.7 | 17.0 % | 18 | 30.4 % | 78 |
| sampled, T 1.0 | 28.0 % | 13 | 37.3 % | 69 |
| *oracle, beam + greedy* | *13.2 %* | *24* | *22.8 %* | *90* |
| *oracle, beam + greedy + T 0.3* | *12.5 %* | *25* | *22.5 %* | *91* |
| *oracle, all six* | *11.4 %* | *29* | *19.6 %* | *97* |
| majority vote, all six | 13.9 % | 21 | 26.1 % | 84 |
| LM pick, all six | 15.9 % | 23 | 25.4 % | 88 |
| LM pick, all six, vote 2 | 15.9 % | 23 | 23.2 % | 91 |
| LM pick, all six, vote 2, length guard 0.15 | 13.8 % | 22 | 24.6 % | 89 |
| LM pick, beam + greedy + T 0.3 | 12.9 % | 24 | 25.0 % | 87 |
| LM pick, beam + greedy + T 0.3, length guard 0.15 | 12.9 % | 24 | 25.7 % | 86 |

The oracle rows (italic) show the most any picker could gain: the candidate with the fewest errors per clip, chosen by looking at the reference.

Cost, on top of the shipped pass:

| | FLEURS (p50) | short (p50) |
|---|---:|---:|
| one extra greedy or sampled decode | 1.02–1.07 s | 0.68–0.71 s |
| LM scoring of the candidates (three runs) | 0.09 s (p95 0.19, max 0.47) | |
| LM scoring (six runs) | 0.10–0.12 s (p95 0.30, max 0.86) | |
| memory | + ~0.6 GB for the model ([polish-06](polish-06.md)), next to large-v3's 2.05 GB | |

## Findings

1. **The headroom is real.** On 194 of 232 clips the six runs disagree, and the best candidate per clip lowers WER by 2 points on FLEURS and 6 on short phrases. Most of that comes from the sampled runs, whose own WER is worse: they get some words right that beam search gets wrong, and many others wrong.
2. **The model cannot find the better candidate.** A summed log-probability prefers shorter text. With all six runs it picked truncated or garbled high-temperature outputs ("Rubin Utappa szerezte a játék", "… közö trainings dynamic text"), so FLEURS got worse (15.9 %). Rewarding agreement and dropping candidates of a different length removes those, but what is left is close to the baseline: 13.8 % on FLEURS, and short phrases go up and down with the setting. The cheapest variant, three runs, gains 0.5 points on FLEURS (12.9 %, 11 clips better, 5 worse) and 0 to 0.7 on short phrases. All weights were chosen on these same clips, so the true gain is likely smaller.
3. **Most misheard words are not among the candidates.** Of the 39 misheard items in `bench/polish/misheard-hu.tsv` (all taken from these clips), the hand-written reference is one of Whisper's six candidates for only 3, and the picker chose it once at most. The errors that matter, "Holnagy" for "Holnap", "Te óvodj kávé" for "Tea vagy kávé", are what the model hears every time. Picking among its guesses cannot fix them; only a better acoustic model can (the fine-tune route, [whisper-04](whisper-04.md), [finetune-01](finetune-01.md)).
4. **The cost is the extra decodes, not the language model.** Scoring takes about 0.1 s. Two more large-v3 passes take about 1.7 s on a sentence and 1.4 s on a short phrase, and running them at the same time would share one GPU. Getting the beam alternatives for free means patching whisper.cpp's decoder to return its final beams, which is not worth it for a 0.5-point gain.

**Recommendation:** do not build N-best rescoring. The sequential memory plan from the round-2 brief (part d) only applied if this showed a gain worth shipping, so it is not run. What is left of round 2 is the fine-tune pair builder and LoRA recipe (prep only, not run).

## Reproduce

```bash
# temporary switch in engine.rs::run (not committed): with PUSMOGO_NBEST_TEMP=t,
#   strategy = Greedy { best_of: 1 }; params.set_temperature(t); params.set_temperature_inc(0.0)
(cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper)
M=~/Library/Application\ Support/Pusmogo/models/ggml-large-v3-q5_0.bin
sets=(bench/audio/fleurs-hu/hu.tsv bench/audio/short/{hu,low,noisy}.tsv)
core/target/release/pusmogo-bench --model "$M" --json /tmp/nb/base.json "${sets[@]}"
PUSMOGO_BEAM=1 core/target/release/pusmogo-bench --model "$M" --json /tmp/nb/greedy.json "${sets[@]}"
for t in 0.3 0.5 0.7 1.0; do
  PUSMOGO_NBEST_TEMP=$t core/target/release/pusmogo-bench --model "$M" --json /tmp/nb/t$t.json "${sets[@]}"
done
# scratch venv: CMAKE_ARGS="-DGGML_METAL=on" uv pip install --no-binary llama-cpp-python llama-cpp-python numpy
python -I scripts/nbest-rescore.py /tmp/nb/{base,greedy,t0.3,t0.5,t0.7,t1.0}.json          # oracle, no model
python -I scripts/nbest-rescore.py --model Qwen3-1.7B-Q4_K_M.gguf --vote 2 --len-guard 0.15 \
    /tmp/nb/{base,greedy,t0.3,t0.5,t0.7,t1.0}.json
```
