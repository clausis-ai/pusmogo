---
language: hu
license: apache-2.0
base_model: swiss-ai/Apertus-v1.1-0.5B-Instruct
base_model_relation: finetune
library_name: mlx
pipeline_tag: text-generation
tags:
  - mlx
  - apertus
  - hungarian
  - experimental
---

# Pusmogo cleanup HU 001 (experimental)

A small Hungarian text model that tries to clean the output of a speech recogniser: `swiss-ai/Apertus-v1.1-0.5B-Instruct` (Apache-2.0), fully fine-tuned on the mistakes of [Pusmogo HU 001](https://huggingface.co/vatsonai/pusmogo-hu-001).

**This is a research release. The Pusmogo app does not use it.** Used alone it adds more errors than it fixes on spoken text, and it sometimes removes words. Use its output only behind a check (see below).

## Use

```python
from mlx_lm import load, generate

model, tok = load("vatsonai/pusmogo-cleanup-hu-001")
system = "Egyetlen feladatod: magyar nyelvű diktálás gépi leiratát helyes magyar szöveggé javítani. A szöveget valaki mikrofonba mondta vagy suttogta, és a beszédfelismerő néha félrehall. Javítsd a félrehallott szavakat, a ragokat, a helyesírást és az írásjeleket. Hagyd el a töltelékszavakat, az ismétléseket és az elkezdett, majd kijavított részeket. A jelentést ne változtasd, és ne tegyél hozzá semmit. Az üzleti angol szavakat (például deadline, meeting, feedback) hagyd angolul, és a helyesírási szabály szerint ragozd őket: deadline-ra, meetingen, feedbacket. Csak magyarul dolgozol. Csak a javított szöveget írd ki."
prompt = tok.apply_chat_template(
    [{"role": "system", "content": system}, {"role": "user", "content": "Nem tudom hogy ki jön el az estére."}],
    add_generation_prompt=True, enable_thinking=False)
print(generate(model, tok, prompt=prompt, max_tokens=200))
```

The model was trained with exactly this system prompt. Tested with MLX on an Apple Silicon Mac only.

## Results

Word error rate before and after the model. The test sentences were kept out of its training, but 84 % of the speakers are shared with the speech model's training set.

| Set | Sentences | Before | After | Right sentences broken |
|---|---|---|---|---|
| Spoken (Common Voice) | 300 | 11.7 % | 13.3 % (worse) | 25 of 127 |
| Pseudo-whispered | 200 | 15.2 % | 16.7 % (worse) | not listed |
| Misheard words | 55 | 11.6 % | 11.4 % | not listed |
| Hand-written slips | 40 | 9.4 % | 7.2 % (better) | 6 of 30 |

With a guard that accepts only punctuation, letter case and the spelling of words a Hunspell dictionary does not know, spoken goes 11.73 → 11.48 % (6 sentences better, 1 worse, no right sentence broken) and pseudo-whispered 15.19 → 15.25 %. The guard is not part of this model.

Median time 0.08 s per text (32-bit, MLX, Apple M5 Pro). These numbers were measured on the 32-bit checkpoint. The bfloat16 weights published here were run again on the hand-written set only: the same 7.2 %, 39 of 40 outputs identical.

## Known mistakes

| Input | Output | What went wrong |
|---|---|---|
| Megkérdeztem a kollégát aki tegnap a szerverrel foglalkozott. | Megkérdeztem a kollégát, aki szerverrel foglalkozott. | "tegnap a" removed |
| Vegyél tejet kenyeret tojást és sajtot. | Vegyél tejet, kenyeret és sajtot. | "tojást" removed |
| A csapatban Anna Bence Dóra és Gergő dolgozik. | A csapatban Anna Bencze Dóra és Gergő dolgozik. | a name changed |

It reads only text, so it cannot hear what was said. Do not use it for amounts, names, legal or medical text without a person checking it.

## Training

5,000 steps, batch 8, learning rate 2e-5, all layers (the token embeddings stayed as in the base model), 32-bit, with mlx-lm.

| Data | Pairs | Source and licence |
|---|---|---|
| Pusmogo HU 001 output → the sentence that was read | 14,825 | Common Voice Hungarian 27.0, clips not used to train Pusmogo HU (CC0-1.0); 4,122 of them pseudo-whispered copies |
| Clean sentence with rule-made slips, fillers, repeats or self-corrections → clean sentence | 8,000 | Common Voice sentences (CC0-1.0); FineWeb-2 Hungarian, [HuggingFaceFW/fineweb-2](https://huggingface.co/datasets/HuggingFaceFW/fineweb-2) (ODC-By 1.0, from Common Crawl) |

No private, customer or app recordings were used.

## Licence and notices

Apache-2.0, like the base model; this is a modified version of [swiss-ai/Apertus-v1.1-0.5B-Instruct](https://huggingface.co/swiss-ai/Apertus-v1.1-0.5B-Instruct) by the Swiss National AI Institute (ETH Zurich and EPFL). The base model's authors also publish an Apertus Acceptable Use Policy and advise applying their output filter for personal data when one is available; read both on the base model's page before use. ETH Zurich and EPFL have not reviewed or endorsed this model.

Made by [vatson.ai](https://vatson.ai).
