# models-02: speech models that fit in 2 GB of memory

Up: [../README.md](../README.md)
Verified: 2026-11-09

Goal: find a Hungarian and English speech model whose whole working memory stays under 2 GB, so that one default works on any Mac, including 8 GB ones. Measurement only: no app or core code changed and no dependency was added. Candidates, as set for this round:

| | candidate | status |
|---|---|---|
| a | `nvidia/nemotron-labs-unified-asr-0.6b` (OpenMDW-1.1), offline mode, 8-bit | **measured below** |
| b | `nvidia/parakeet-tdt-0.6b-v3` (CC-BY-4.0) | Hungarian is in its 25-language list, but it cannot be told which language to write ([bakeoff-04](bakeoff-04.md), bfloat16); **8-bit measured below** |
| c | `nvidia/canary-1b-v2` (CC-BY-4.0) at 4-bit | **measured below** (8-bit is in [models-01](models-01.md)) |

Baselines are quoted from earlier runs with the same scorer: large-v3 and turbo q5_0 ([models-01](models-01.md), [whisper-01](whisper-01.md), [fleurs-en-01](fleurs-en-01.md) with the English normaliser of 895dd7e), canary 8-bit ([models-01](models-01.md)), Parakeet v3 ([bakeoff-04](bakeoff-04.md)). The scorer's text cleanup has gained a few rules since, so a quoted row could move by a word; none of them touch FLEURS-style prose.

## (a) Nemotron unified ASR 0.6B

**Answer: fast and small, but the weakest Hungarian model measured so far.** FLEURS Hungarian 18.0 % WER (turbo 17.0 %, large-v3 13.4 %), short Hungarian phrases 48.6 % (turbo 35.5 %, large-v3 23.9 %), question marks on 10 of 44 questions, and two Cyrillic outputs although Hungarian was forced. English is on par with turbo (FLEURS 5.8 % vs 5.9 %) at a tenth of the time. Peak memory 1.35 GB for clips up to 30 s with the runtime's buffer cache off (1.78 GB with its default cache), but 3.38 GB when a 60–76 s dictation is decoded in one pass.

### How it was run

- Model: `mlx-community/nemotron-labs-unified-asr-0.6b-8bit` (NVIDIA's weights converted to Apple's MLX; linear layers 8-bit, the rest bfloat16; 744 MB), Hugging Face commit `b7108f13`, SHA-256 of `model.safetensors` `cdf9255e…4d9d21` checked against the repository's published hash. Stored under `~/Library/Application Support/Pusmogo/m2/`, never committed. NVIDIA's original `.nemo` file is 2.5 GB, over this round's 2 GB per-file limit.
- Runtime: `mlx-audio` 0.6.4 (MIT) from PyPI in a scratch virtual environment outside the repo. That release cannot load this checkpoint: its conformer only knows the streaming-only sibling model (layer norm in the convolution module, causal subsampling). A scratch patch of about 20 lines, not committed, swapped in batch norm with the checkpoint's running statistics and NeMo's symmetric subsampling padding. Check of the patch: on five English FLEURS clips the output matched the references word for word apart from number formatting, and the 100-clip English set scores 5.8 % WER, in line with NVIDIA's published English figures for this model.
- Decoding: offline mode (full attention context, `att_context_size [-1, -1]`), greedy RNN-T, language prompt `hu` (prompt index 23) or `en`; each clip decoded whole (`chunk_duration=None`). Float32 audio and activations (the runtime default).
- Time per clip = `generate()` wall time after one untimed warm-up; model load 0.06–0.11 s from a warm file cache.
- Scoring: `pusmogo-bench --hyp-file`, so the text goes through the same `respell` + `clean()` and normaliser as the Whisper rows. Like canary in models-01, these rows have no speech detector, energy gate or Whisper guards in front.
- Memory: `/usr/bin/time -l` peak memory footprint of the whole Python process (model, MLX buffers, interpreter).
- Apple M5 Pro. Synthetic speech (macOS TTS) is a proxy for a real voice; FLEURS is real read speech.

### Results

| set | clips | turbo WER % | large-v3 WER % | canary 8-bit WER % | Parakeet v3 WER % | **unified WER %** | unified p50 s | unified worst s |
|---|---|---|---|---|---|---|---|---|
| FLEURS hu (real speech) | 100 | 17.0 | **13.4** | 12.4 | 16.8 | 18.0 | 0.11 | 0.25 |
| FLEURS hu, CER | 100 | 4.1 | **3.5** | 3.6 | 4.5 | 4.7 | | |
| short hu | 44 | 31.5 | **18.5** | 21.7 | 35.9 | 43.5 | 0.03 | 0.30 |
| short low (lower, faster voice) | 44 | 25.0 | **13.0** | 30.4 | 38.0 | 44.6 | 0.02 | 0.04 |
| short noisy (fan, babble, keys) | 44 | 50.0 | **40.2** | 41.3 | 66.3 | 57.6 | 0.03 | 0.05 |
| short, hu + low + noisy | 132 | 35.5 | **23.9** | 31.2 | 46.7 | 48.6 | | |
| short app (capture path) | 44 | 31.5 | **18.5** | 22.8 | 39.1 | 44.6 | 0.03 | 0.04 |
| synth hu sentences | 60 | **6.8** | – | – | 8.6 | 8.4 | 0.06 | 0.08 |
| synth mixed HU/EN | 30 | **18.3** | – | – | 24.2 | 24.7 | 0.05 | |
| long hu, 58–76 s, whole clip in one pass | 8 | 18.4 | **13.9** | 43.6 | – | 23.1 | 0.49 | 0.59 |
| name set, no hints | 6 | – | **26.4** | 34.0 | – | 37.7 | | |
| name set, matching context | 6 | – | **13.2** (prompt) | 30.2 (respell only) | – | 34.0 (respell only) | | |
| inflected names, no hints | 8 | – | **27.7** | 31.9 | – | 34.0 | | |
| inflected names, matching context | 8 | – | **0.0** (prompt) | 19.1 (respell only) | – | 31.9 (respell only) | | |
| whispered short (pseudo-whisper) | 44 | 89.1 | 87.0 | 90.2 | – | **84.8** | | |
| whispered FLEURS (pseudo-whisper) | 100 | 47.2 | **42.3** | 48.7 | – | 55.7 | | |
| silence + noise beds (clips with invented text) | 14 | 0 | 0 | 1 | 0 | **0** | | |
| FLEURS en (real speech) | 100 | 5.9 | **4.8** | – | – | 5.8 (CER 2.5) | 0.09 | |
| synth en | 20 | **0.0** | – | – | 1.6 | 1.6 | 0.05 | |

Other checks:

| | unified 0.6B, 8-bit, MLX |
|---|---|
| download | 744 MB (4-bit: 472 MB, not measured) |
| peak memory footprint | **1.35 GB** over FLEURS + short sets (clips ≤ 30 s) with the buffer cache off (`mx.set_cache_limit(0)`, text identical on 276 of 276 clips); with the runtime default cache 1.78 GB over the same sets, 1.75 GB over English FLEURS; **3.38 GB** once the 60–76 s clips are decoded whole (full attention grows with the square of the length) |
| peak resident memory | 0.87–0.90 GB |
| one-word or short clips with no output | 13 of 176 ("Rablás.", "Szia.", "Liszt.", "Hal.", "Lámpa.", "Hajó.", "Rossz a hang.", …); large-v3 and canary: 1 of 176 |
| wrong script with Hungarian forced | 2 of 176 short clips, both over babble ("Руха.", "Твой войнуш.") |
| question marks kept on the 44 questions in the short sets | 10 (large-v3 32, canary 26) |
| punctuation and capitals | yes: all 100 FLEURS outputs end with a full stop or other mark, 99 start with a capital, 79 have a comma |
| vocabulary prompt (Dictionary, context hints) | no: only after-the-fact respelling, which recovers little (name set 37.7 → 34.0 %) |
| streaming | yes, the same weights decode in 0.16–2.08 s chunks (not measured here; NVIDIA reports Hungarian 14.3 % offline vs 19.7 % at 1.12 s latency, averaged over FLEURS, MCV and MLS) |
| licence | OpenMDW-1.1: use, change and share without restriction; no restriction of any kind on outputs (so benchmarking and training on its transcripts are allowed); a redistributed copy must carry the licence and NVIDIA's notices; rights end for anyone who sues claiming the model infringes a patent or copyright |
| runtime for the app | MLX is far over the 10 MB bundle ([bakeoff-04](bakeoff-04.md): 21.7 MB library + 214 MB shader file). Public Core ML conversions exist (int8 encoder ~570 MB), which would use the system's Core ML instead; not measured |

### Findings

1. **Hungarian accuracy is below turbo everywhere that matters.** FLEURS 18.0 vs 17.0 %, short phrases 48.6 vs 35.5 %. Typical short-phrase misses are dropped or clipped single words ("Lop" for "Lopás", "Lám" for "Lámpa", nothing for "Rablás."), and one-letter slips ("Regel." for "Reggel.", "Hello!" for "Helló.").
2. **The language prompt does not hold on short noisy input.** Two clips over babble came out in Cyrillic with `hu` forced. Rarer than Parakeet v3 with no prompt (8 of 176), but still a wrong-script output a Hungarian user could see.
3. **It is fast**: 0.03 s for a short phrase, 0.11 s for a FLEURS sentence, under 0.6 s for a whole minute of speech. Like canary, it would meet the 0.30 s short-pass target that Whisper cannot reach ([speed-01](speed-01.md)).
4. **Memory fits 2 GB only with bounded input.** Long dictations must be cut at pauses (as our streaming session already does for Whisper) or decoded in streaming mode; whole 76 s clips pushed the footprint to 3.4 GB. Half-precision activations would lower both numbers (not measured).
5. **It invents nothing on silence or noise** (0 of 14), better than canary and large-v3 without our guards.
6. **English is on par with turbo**, and whispered short phrases are slightly less bad than every other model (84.8 %), but whispered FLEURS is much worse (55.7 vs 42.3 %).

### Recommendation

Do not adopt it for Hungarian. It loses to turbo, the current 8 GB default, on every Hungarian set except whispered short phrases, and it has no vocabulary prompt. If a sub-2 GB fast path is wanted, canary-1b-v2 remains the stronger Hungarian candidate (FLEURS 12.4 %, short 31.2 %); its 4-bit memory and accuracy are the next measurement in this report.

## (c) canary-1b-v2 at 4-bit

**Answer: the best Hungarian model that fits 2 GB, at 1.34 GB peak, but still not better than large-v3 where it counts.** FLEURS Hungarian 13.1 % WER (8-bit 12.4 %, large-v3 13.4 %, turbo 17.0 %), short phrases 33.7 % (8-bit 31.2 %, turbo 35.5 %, large-v3 23.9 %), at 0.14 s per FLEURS sentence and 0.03 s per short phrase. Going from 8 to 4 bits costs under one point on sentences and about two on short phrases. Against turbo, today's model for 8 GB Macs, it wins on real sentences and clean short phrases and loses on the lower voice, noise, mixed Hungarian/English and whispers; it keeps canary's known costs (no vocabulary prompt, "Köszönöm." on one second of silence, whispers no better).

### How it was run

- Model: `TechHara/canary-1b-v2-mlx-q4` (NVIDIA's weights converted to MLX; 4-bit, group size 32; 755 MB), Hugging Face commit `dd7ad732`, SHA-256 of `model.safetensors` `3deddc1b…44363f4` checked against the repository's published hash. The repository ships its vocabulary only as `tokens.json`, which `mlx-audio` cannot read, so the SentencePiece `tokenizer.model` (504 KB) was taken from `eelcor/canary-1b-v2-mlx` (commit `4f13fe9a`); all 16,384 pieces match `tokens.json` one for one. Both under `~/Library/Application Support/Pusmogo/m2/`, never committed. The repository states no licence of its own; the weights are NVIDIA's CC-BY-4.0 model.
- Runtime: the same `mlx-audio` 0.6.4 virtual environment as (a), unpatched (its `canary` model type loads this checkpoint as is). Source and target language forced (`hu` or `en`), punctuation and capitals on, greedy decoding, bfloat16 activations. Clips longer than 30 s are cut by the runtime at the quietest point near each 30 s mark (the long set: three pieces per clip).
- Time per clip, scoring and memory measurement as in (a). MLX's buffer cache was turned off (`mx.set_cache_limit(0)`), see finding 1; the text is identical with and without it (380 of 380 clips).

### Results

| set | clips | turbo WER % | large-v3 WER % | canary 8-bit WER % | **canary 4-bit WER %** | 4-bit p50 s | 4-bit worst s |
|---|---|---|---|---|---|---|---|
| FLEURS hu (real speech) | 100 | 17.0 | 13.4 | **12.4** | 13.1 | 0.14 | 0.41 |
| FLEURS hu, CER | 100 | 4.1 | **3.5** | 3.6 | 3.8 | | |
| short hu | 44 | 31.5 | **18.5** | 21.7 | 20.7 | 0.03 | 0.07 |
| short low (lower, faster voice) | 44 | 25.0 | **13.0** | 30.4 | 28.3 | 0.03 | |
| short noisy (fan, babble, keys) | 44 | 50.0 | **40.2** | 41.3 | 52.2 | 0.04 | |
| short, hu + low + noisy | 132 | 35.5 | **23.9** | 31.2 | 33.7 | | |
| short app (capture path) | 44 | 31.5 | **18.5** | 22.8 | 20.7 | 0.03 | |
| synth hu sentences | 60 | **6.8** | – | – | **6.8** | 0.08 | 0.10 |
| synth mixed HU/EN | 30 | **18.3** | – | – | 21.9 | 0.06 | 0.08 |
| long hu, 58–76 s, cut at quiet points into ≤ 30 s pieces | 8 | – | – | 14.6 (≤ 28 s pieces) | 16.8 | 0.98 per clip | 1.12 |
| name set, no hints | 6 | – | 26.4 | 34.0 | 34.0 | | |
| name set, matching context | 6 | – | **13.2** (prompt) | 30.2 (respell only) | 34.0 (respell only) | | |
| inflected names, no hints | 8 | – | 27.7 | 31.9 | 34.0 | | |
| inflected names, matching context | 8 | – | **0.0** (prompt) | 19.1 (respell only) | 25.5 (respell only) | | |
| whispered short (pseudo-whisper) | 44 | 89.1 | **87.0** | 90.2 | 92.4 | | |
| whispered FLEURS (pseudo-whisper) | 100 | 47.2 | **42.3** | 48.7 | 64.5 | | |
| silence + noise beds (clips with invented text) | 14 | 0 | 0 | 1 | 1 ("Köszönöm." on 1 s of silence) | | |
| FLEURS en (real speech) | 100 | 5.9 | **4.8** | – | 5.7 (CER 2.9) | 0.15 | 0.26 |
| synth en | 20 | **0.0** | – | – | 3.2 | 0.08 | |

Other checks:

| | canary-1b-v2, 4-bit, MLX |
|---|---|
| download | 755 MB (+ 504 KB tokenizer) |
| peak memory footprint, buffer cache off | **1.34 GB** over FLEURS + short + synth sets (clips ≤ 30 s); 1.34 GB over English; 1.43 GB over the long set (30 s pieces) |
| peak memory footprint, runtime default cache | 10.3 GB over the same Hungarian sets: MLX keeps freed GPU buffers for reuse, and clips of many lengths fill that cache. An app on this runtime must cap it |
| peak resident memory | 0.89–0.98 GB |
| model load (warm file cache) | 0.5–0.8 s |
| one-word or short clips with no output | 1 of 176 (one babble clip), as large-v3 and canary 8-bit |
| wrong script with Hungarian forced | 0 of 176 short clips, 0 of 158 name and whispered clips |
| question marks kept on the 33 questions in the hu, low and noisy short sets | 21 |
| punctuation and capitals | yes: all 100 FLEURS outputs end with a mark, 99 start with a capital, 78 have a comma |
| vocabulary prompt (Dictionary, context hints) | no: only after-the-fact respelling (name sets 34.0 → 34.0 %, inflected names 34.0 → 25.5 %) |
| licence | CC-BY-4.0 (NVIDIA's model card): use, change and share, including commercially, with attribution in NOTICE; nothing restricts outputs |
| runtime for the app | as in (a): MLX is far over the 10 MB bundle. A ggml build (transcribe.cpp, [models-01](models-01.md)) or Core ML conversions are the routes; a 4-bit GGUF was not measured |

### Findings

1. **Memory fits 2 GB with room to spare, once the runtime's buffer cache is capped.** 1.34 GB peak with the cache off, against 1.36 GB for the 8-bit GGUF under transcribe.cpp ([models-01](models-01.md)) and 1.78 GB for Nemotron in (a). The (a) model, rerun with the cache off, peaks at 1.35 GB too. Without a cap the same run reached 10.3 GB: any MLX-based path in the app must set one.
2. **Four bits cost little on sentences, more on noisy short phrases.** FLEURS 12.4 → 13.1 %, short phrases 31.2 → 33.7 %, almost all of it from the noisy set (41.3 → 52.2 %); clean short phrases even came out slightly better (21.7 → 20.7 %). Whispered FLEURS got much worse (48.7 → 64.5 %).
3. **Against turbo, the model that 8 GB Macs use today:** better on FLEURS (13.1 vs 17.0 %), clean short phrases (20.7 vs 31.5 %) and English FLEURS (5.7 vs 5.9 %), worse on the lower voice (28.3 vs 25.0 %), noise (52.2 vs 50.0 %) and mixed Hungarian/English (21.9 vs 18.3 %); overall on short phrases 33.7 vs 35.5 %. About 20 times faster per clip and 1.34 GB vs 0.82 GB of memory.
4. **Against large-v3, the Hungarian default above 8 GB:** level on FLEURS, 10 points behind on short phrases, and no vocabulary prompt, so names on screen and Dictionary entries lose most of their effect (inflected names 25.5 vs 0.0 % with matching context).

### Recommendation

Within 2 GB, canary-1b-v2 at 4-bit is the strongest Hungarian candidate measured: it beats Nemotron (a) on every Hungarian set except the two whispered ones, and turbo on real sentences and clean short phrases, at a fraction of the time. It does not replace large-v3, and on short noisy phrases (common in real use) it is no better than turbo. If a single sub-2 GB model for every Mac is still the goal, it is the one to size a shippable runtime for (a 4-bit ggml build against the 10 MB bundle), with two costs to accept: Dictionary and context hints work only as respelling, and a guard is needed for very short input. Part (b) below closes the round.
## (b) Parakeet TDT 0.6B v3 at 8-bit

**Answer: fits 2 GB for clips up to 30 s (1.67 GB), the fastest model measured, but it cannot be held to Hungarian.** It picks the language itself, and 12 of 276 Hungarian clips came out in Cyrillic ("Риж.", "Ланпа."). Short Hungarian phrases 42.0 % WER (turbo 35.5 %, canary 4-bit 33.7 %), FLEURS 16.2 % (turbo 17.0 %).

### How it was run

- Model: `animaslabs/parakeet-tdt-0.6b-v3-mlx-8bit` (NVIDIA's CC-BY-4.0 weights converted to MLX; 8-bit, group size 64; 908 MB), Hugging Face commit `18498133`, SHA-256 of `model.safetensors` `3b11b8e2…3b1ab2` checked against the repository's published hash. Stored under `~/Library/Application Support/Pusmogo/m2/`, never committed.
- Runtime: the same `mlx-audio` 0.6.4 environment, unpatched (`parakeet` model type). No language argument exists; the model decides. Greedy TDT decoding, punctuation and capitals on. The long set's 58–76 s clips are decoded whole by the runtime.
- Time per clip, scoring and memory as in (a) and (c); buffer cache off unless stated.

### Results

| set | clips | turbo WER % | large-v3 WER % | canary 4-bit WER % | **Parakeet 8-bit WER %** | p50 s | p95 s |
|---|---|---|---|---|---|---|---|
| FLEURS hu | 100 | 17.0 | 13.4 | 13.1 | 16.2 (CER 4.3) | 0.08 | 0.12 |
| short hu | 44 | 31.5 | **18.5** | 20.7 | 28.3 | 0.02 | 0.03 |
| short low | 44 | 25.0 | **13.0** | 28.3 | 34.8 | 0.02 | 0.03 |
| short noisy | 44 | 50.0 | **40.2** | 52.2 | 63.0 | 0.02 | 0.04 |
| short, hu + low + noisy | 132 | 35.5 | **23.9** | 33.7 | 42.0 | 0.02 | 0.03 |
| long hu, 58–76 s, whole | 8 | 19.5 (in app) | **15.2** (in app) | 16.8 (≤ 30 s pieces) | 22.7 | 0.37 per clip | 0.41 |

Other checks:

| | Parakeet TDT 0.6B v3, 8-bit, MLX |
|---|---|
| download | 908 MB |
| peak memory footprint, buffer cache off | **1.67 GB** over FLEURS + short sets (clips ≤ 30 s); 2.53 GB over the long set decoded whole |
| peak memory footprint, runtime default cache | 14.8 GB over the same sets |
| model load (warm file cache) | 0.5 s |
| wrong script (Cyrillic) | **12 of 276** (2 clean short phrases, 10 noisy); bfloat16 in bakeoff-04: 8 of 176 |
| clips with no output | 0 of 276 |
| question marks kept on the 33 questions in the hu, low and noisy short sets | 16 (canary 4-bit 21) |
| punctuation and capitals | yes: 100 of 100 FLEURS outputs end with a mark, 99 start with a capital, 74 have a comma |
| vocabulary prompt | no |
| licence | CC-BY-4.0, as canary |

### Findings

1. **No way to force Hungarian, so it is out.** With 12 of 276 Hungarian clips in Cyrillic, about one short noisy phrase in eight would come out in the wrong script; no rule after the fact can repair that.
2. **8-bit costs nothing on sentences** (FLEURS 16.8 → 16.2 % against the bfloat16 run in bakeoff-04) and is the fastest model in the round (0.02 s per short phrase, 0.08 s per sentence).
3. **Memory: 1.67 GB, the most of the three under 2 GB**, and 14.8 GB if the MLX cache is left on, the same lesson as (c).

## Round summary and recommendation

| under 2 GB (clips ≤ 30 s, cache off) | peak | FLEURS hu WER % | short hu + low + noisy WER % | language forced | wrong script / 276 | vocabulary prompt |
|---|---|---|---|---|---|---|
| turbo q5_0 (whisper.cpp, today's 8 GB model) | 0.82 GB | 17.0 | 35.5 | yes | 0 | **yes** |
| (a) Nemotron unified 0.6B, 8-bit | 1.35 GB | 18.0 | 48.6 | yes | 2 | no |
| (b) Parakeet TDT 0.6B v3, 8-bit | 1.67 GB | 16.2 | 42.0 | **no** | 12 | no |
| (c) canary-1b-v2, 4-bit | **1.34 GB** | **13.1** | **33.7** | yes | 0 | no |
| reference: large-v3 q5_0, above 2 GB | 2.05 GB | 13.4 | 23.9 | yes | 0 | yes |

**Recommendation: keep Whisper for v0.1; canary-1b-v2 is the only sub-2 GB model worth a next step.** None of the three closes the gap to large-v3 on short Hungarian phrases, which are most of real dictation, and none takes a vocabulary prompt, so Dictionary entries and names on screen would only work as respelling. Canary at 4-bit is the one that beats turbo on Hungarian (FLEURS 13.1 vs 17.0 %, short 33.7 vs 35.5 %) at about 20 times the speed, so if one model for every Mac stays the goal, the next step is sizing a 4-bit ggml canary runtime against the 10 MB bundle and measuring what respelling-only Dictionary support costs on real dictations. Nemotron (weaker Hungarian, drops one-word clips) and Parakeet (no forced language) are not recommended. Every MLX path must cap the buffer cache: all three models ran to 10–15 GB without it.

## Reproduce

```bash
uv venv -p 3.12 /tmp/m2/.venv && uv pip install --python /tmp/m2/.venv/bin/python mlx-audio==0.6.4
export HF_HOME="$HOME/Library/Application Support/Pusmogo/m2/hf"
# scratch runner: patch the conformer (batch norm, symmetric subsampling padding), load
# mlx-community/nemotron-labs-unified-asr-0.6b-8bit, then per manifest clip read the 16 kHz WAV and run
# model.generate(audio, language="hu", chunk_duration=None, att_context_size=[-1, -1]);
# write id<TAB>seconds<TAB>text to /tmp/m2/nu-hu.tsv
/usr/bin/time -l python -I run.py mlx-community/nemotron-labs-unified-asr-0.6b-8bit hu /tmp/m2/nu-hu.tsv bench/audio/fleurs-hu/hu.tsv bench/audio/short/{hu,low,noisy,app}.tsv
core/target/release/pusmogo-bench --hyp-file /tmp/m2/nu-hu.tsv bench/audio/fleurs-hu/hu.tsv
# (b): same venv, no patch, model type parakeet, no language argument, mx.set_cache_limit(0)
# (c): same venv, no patch; model dir = the q4 snapshot plus tokenizer.model; model.generate(audio, language="hu") with mx.set_cache_limit(0)
python3 scripts/pseudo-whisper.py bench/audio/short/hu.tsv /tmp/w1/short-w   # whispered sets, as in whisper-01
```
