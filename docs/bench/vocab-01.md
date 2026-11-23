# Built-in tech vocabulary in the engine prompt (vocab-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

**Result: not shipped as a default.** A built-in prompt of Hungarian business and tech terms cuts WER on the mixed-language set from 18.7 % to 13.2 %, but makes general Hungarian speech (FLEURS) worse: 17.5 % → 18.1 % for the best variant, 19.5 % for a plain word list. The engine keeps an empty prompt unless the user adds dictionary entries. A per-user opt-in is a product decision and stays open.

## Why we tried it

Hungarians dictating at work mix English terms into Hungarian ("Mergeltem a feature branchet"). Whisper's `initial_prompt` is decoded as if it were the text before the clip, so terms in it are more likely to come out with the right spelling. The personal dictionary already feeds its spellings to the prompt; this tested a built-in list for everyone.

## What we measured

Turbo q5 (`ggml-large-v3-turbo-q5_0.bin`), Apple Silicon with Metal, warm engine, greedy decoding (deterministic, so every changed output is caused by the prompt). `pusmogo-bench --prompt-file F`. Prompt files: [`bench/prompts/`](../../bench/prompts/).

| prompt | mixed WER % | hu (synth) WER % | en WER % | FLEURS WER % | FLEURS joined % | FLEURS p50 s |
|---|---|---|---|---|---|---|
| none (current) | 18.7 | 7.3 | 0.0 | 17.5 | 14.3 | 0.73 |
| `tech-list.txt`: 86 terms, comma list | 13.7 | 6.8 | 0.0 | 19.5 | 15.5 | 0.78 |
| list without terms that occur in the mixed references (37 terms) | 18.3 | 7.0 | 0.0 | not run | | |
| `tech-prose-hu.txt`: the same kind of terms in five natural Hungarian sentences | 13.2 | 7.0 | not run | 18.1 | 14.8 | 0.77 |

Silence stayed 6/6 empty and no FLEURS clip went empty in any run. Latency grows by 30 to 50 ms because the prompt tokens are decoded too.

The held-out row shows the gain is almost entirely on the listed words: a prompt helps the terms it names and little else. The mixed set was written by us and shares many terms with any tech list, so its 5-point gain overstates what a user with different jargon would see.

## What goes wrong on general speech

With the comma list, 53 of 100 FLEURS outputs changed. Commas and quotation marks are dropped (the prompt has a comma after every word, so commas stop marking clauses), ordinals are spelled out ("7." → "hetedik"), one clip is transcribed twice in a row, and one clip came out in Icelandic ("A rendaður sig þarinn út þín…"). Rare words also shift ("csereszlen" → "ceresztény", "mindenki" → "mindenők").

The prose prompt keeps punctuation and numbers stable (46 outputs changed), but small spelling shifts remain ("növelheti" → "nevelheti", "által épített" → "általépített"), and the net is +0.6 WER points.

## What this means

- A fixed prompt is not free: it trades general accuracy for the named terms. That fits a user who chose it (dictionary entries, or an opt-in "tech vocabulary" switch), not every user.
- If a prompt is used, write it as natural, punctuated Hungarian text, never as a bare comma list. This applies to the personal dictionary too: `Dictionary::prompt()` currently joins entries with commas; a long dictionary should be checked against FLEURS.
- Mixed-set errors that remain with the prompt are mostly TTS pronunciation of English loanwords ("branchet" → "Brannhet"), which a prompt cannot fix and real voices may not have.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper && cd ..
core/target/release/pusmogo-bench --prompt-file bench/prompts/tech-prose-hu.txt \
  bench/audio/synth/mixed.tsv bench/audio/synth/hu.tsv
core/target/release/pusmogo-bench --prompt-file bench/prompts/tech-prose-hu.txt bench/audio/fleurs-hu/hu.tsv
```
