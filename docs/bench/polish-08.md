# polish-08: correction pairs and a LoRA recipe for a small Hungarian polish model (round 2, part c)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08 · Script: `scripts/make-polish-pairs.py` · Smoke set: the 50 FLEURS Hungarian dev sentences outside the bench, through large-v3 and turbo · **No model was trained.**

## Why

Untrained small models do not help as a polish pass: in [polish-05](polish-05.md) to [polish-07](polish-07.md) they add commas but fix 0 of 39 misheard words, and picking among Whisper's own candidates gains at most 0.5 WER points. The remaining route for a tiny model is to teach it Whisper's own Hungarian mistakes: train on pairs of what the app would type (Whisper's output after the core's `clean()`) and what was said (the human reference). This step builds those pairs and writes down how to train on them later, on a Mac, with nothing bought.

## What the script builds

`scripts/make-polish-pairs.py <out-dir> --runs <bench.json>...` reads `pusmogo-bench --json` runs over training audio and writes `train.jsonl`, `valid.jsonl` and `test.jsonl` in the chat format MLX-LM's trainer reads: `{"messages": [system, user, assistant]}`, with the user turn the app's text and the assistant turn the reference. The system prompt is one short Hungarian sentence; the behaviour has to come from the pairs, and every prompt token is paid on each dictation.

| rule | why |
|---|---|
| drop pairs with no output | nothing to correct |
| drop pairs above `--max-wer` (0.5) | a decode that far off is an invention or the wrong audio; training on it would teach the model to write text it was not given |
| right-as-is pairs at most `--keep-share` (30 %) of the set | the model must learn to leave right text alone, without learning only that |
| split by sentence (hash of the normalised reference) | every take of a sentence, from every model run, lands on the same side |
| drop any pair whose reference is in `bench/audio/*/*.tsv`, `bench/polish/*.tsv` or `--exclude`; refuse a run over bench clip ids unless `--allow-bench` | the model must never train on what the bench scores |
| refuse an output folder inside the repository | the pairs hold dataset text |

`stats.json` counts every filter and splits each side into `keep`, `fix_words` (a word differs) and `fix_format_only` (only punctuation or case differs), with the WER of the kept pairs, the model files and the licences given with `--licence`. Several runs can go in together (for example large-v3 and turbo): they give different mistakes on the same sentence, which is more to learn from.

## Smoke run

FLEURS Hungarian dev minus the 100 bench sentences leaves 50 clips (CC-BY-4.0). large-v3 q5_0: 16.6 % WER on them, p50 1.34 s; turbo q5_0: 14.2 %, p50 0.75 s.

| check | result |
|---|---|
| both runs in | 100 pairs; 1 dropped above 0.5 WER; 0 bench overlap |
| split (10 % valid, 10 % test for the smoke) | train 79 (14 keep, 59 words, 6 format only), valid 10, test 10 |
| WER of the kept pairs | 15.0 % |
| rerun | all three files byte-identical |
| a run over the bench's FLEURS clips | refused (exit 2); with `--allow-bench` all 232 pairs dropped as bench overlap |
| output folder in the repository | refused (exit 2) |

Typical pairs, Whisper → reference: "felnyőtt" → "felnőtt", "rohan felszerelést … könyges" → "rohamfelszerelést … könnygáz", "Fimipino rendor … fogjul" → "filippínó rendőr … foglyul", "surlódás" → "súrlódás". These are the misheard-word errors a text-only pass could not fix untrained ([polish-06](polish-06.md)).

## Findings

1. **The references teach their own style, mistakes included.** FLEURS references are transcriptions with their own slips and habits: "A mrémült XVI. Lajost" (a typo and Roman numerals where the app writes "16."), "sejtet" for "sejtett", "Madam Elizabethet", "Ebola vírusos" where Whisper's "ebolavírusos" is the standard spelling, "1789 október" without the ordinal dot, and 6 of the 150 dev sentences without a final full stop. A model trained on them learns those too. Before a real run, check the references: Common Voice Hungarian (CC0, read prompts written as sentences) is likely cleaner, and pairs whose reference fails a simple check (no final punctuation, Roman numerals the app never types) should be dropped or fixed by hand.
2. **Most of the signal is words, not punctuation.** 59 of 79 training pairs differ in at least one word, 6 only in punctuation or case: the cleanup rules already match FLEURS' punctuation closely, so a fine-tuned model would mostly learn word fixes, which is the part [polish-06](polish-06.md) showed the untrained models cannot do.
3. **The same safety check still applies.** A trained model can still change a right word that sounds like a wrong one. The sound-alike check from [polish-06](polish-06.md) and the meaning-drift guard ([polish-05](polish-05.md)) stay in front of anything it types.
4. **Scale.** FLEURS Hungarian train is 3,095 recordings of 1,497 sentences (9.3 h); through two models that is about 6,000 pairs before filtering. Common Voice Hungarian adds tens of thousands of short read sentences. Both are well within what a LoRA run needs.

## Training recipe (prepared, not run)

Run on an Apple Silicon Mac with 32 GB or more; nothing here spends money. MLX-LM was not installed on the test machine, so check the flags below against `mlx_lm.lora --help` of the version you install.

1. **Audio.** `scripts/fetch-fleurs.py --lang hu --split train --every-recording --count 2000 --out /tmp/ft/fleurs-train` (1.7 GB archive, CC-BY-4.0); Common Voice Hungarian (CC0) needs a manual download from the dataset site and a manifest in the bench format.
2. **Whisper outputs.** `pusmogo-bench --model <large-v3 or turbo q5_0> --json /tmp/ft/<model>.json /tmp/ft/fleurs-train/hu.tsv`, once per shipped model (about 1 s per clip with large-v3, 0.6 s with turbo).
3. **Pairs.** `scripts/make-polish-pairs.py /tmp/ft/pairs --runs /tmp/ft/*.json --licence "FLEURS: CC-BY-4.0" --licence "Common Voice: CC0-1.0"`; read 50 pairs by hand and fix or drop bad references (finding 1).
4. **Train.** In a scratch venv: `pip install mlx-lm`, then
   `mlx_lm.lora --model Qwen/Qwen3-0.6B --train --data /tmp/ft/pairs --fine-tune-type lora --num-layers 16 --batch-size 8 --iters 2000 --learning-rate 1e-4 --mask-prompt --adapter-path /tmp/ft/adapters`
   (`--mask-prompt` trains only on the reference side). Same for `utter-project/EuroLLM-1.7B-Instruct`. Both are Apache-2.0. Stop on the lowest validation loss.
5. **Check before anything else.** `mlx_lm.lora --model … --adapter-path /tmp/ft/adapters --data /tmp/ft/pairs --test` gives the test loss; the real gate is `scripts/polish-llm.py` and `scripts/polish-misheard.py` with the fused model, so the numbers compare with [polish-05](polish-05.md) and [polish-06](polish-06.md).
6. **Ship format.** `mlx_lm.fuse --model … --adapter-path /tmp/ft/adapters --save-path /tmp/ft/fused --dequantize`, then llama.cpp's `convert_hf_to_gguf.py` and `llama-quantize … Q4_K_M`, the format the earlier rounds measured.

**Gate for proposing it** (the round-2 gate): clear gain on the real clips and the misheard set, zero meaning changes passed by the safety check, added wait p50 ≤ 0.5 s warm, peak footprint ≤ 3.5 GB with Whisper on a 16 GB Mac. An opt-in setting at most; no default changes.

## Reproduce

```bash
python3 -I scripts/fetch-fleurs.py --lang hu --split dev --count 160 --out /tmp/p8/dev
cut -f1 bench/audio/fleurs-hu/hu.tsv > /tmp/p8/benchids
awk -F'\t' 'NR==FNR{b[$1]=1;next} !($1 in b)' /tmp/p8/benchids /tmp/p8/dev/hu.tsv > /tmp/p8/dev/rest.tsv
M=~/Library/Application\ Support/Pusmogo/models
for m in large-v3 large-v3-turbo; do
  core/target/release/pusmogo-bench --model "$M/ggml-$m-q5_0.bin" --json /tmp/p8/$m.json /tmp/p8/dev/rest.tsv
done
python3 -I scripts/make-polish-pairs.py /tmp/p8/pairs --runs /tmp/p8/large-v3.json /tmp/p8/large-v3-turbo.json \
  --valid-share 0.1 --test-share 0.1 --licence "FLEURS: CC-BY-4.0"
cat /tmp/p8/pairs/stats.json
```
