# Large personal dictionaries and Whisper's prompt limit (dict-02)

Up: [../README.md](../README.md)
Verified: 2026-11-07

**Result: shipped.** whisper.cpp keeps only the last 224 prompt tokens (half of the 448-token text context) and silently drops the rest from the front. A personal dictionary of about 60 entries already fills that budget; beyond it Whisper cut off the `Gyakori szavak:` label and the oldest entries, often in the middle of a word, and general accuracy fell. The core now fits the dictionary prompt to the budget itself (`Dictionary::prompt_fitting`, with the engine's own tokenizer): the label stays and the oldest spellings are left out of the prompt. Their spoken → written replacements still apply in cleanup.

## What we measured

Turbo q5, Metal, warm engine, greedy decoding, 100 FLEURS Hungarian clips, all runs in one session. Dictionaries: the 20 entries of [dict-01](dict-01.md) plus 40 and then 20 more realistic names (services, Hungarian towns and offices, tax terms, team jargon). Files: [`bench/prompts/dict60.txt`](../../bench/prompts/dict60.txt), [`dict80.txt`](../../bench/prompts/dict80.txt) and their `-label` prompt forms.

| dictionary | prompt tokens | WER % | joined % | CER % | p50 s | p95 s | outputs changed |
|---|---|---|---|---|---|---|---|
| none | 0 | 17.5 | 14.3 | 4.5 | 0.73 | 0.79 | |
| 20 entries | 75 | 17.7 | 14.2 | 4.5 | 0.74 | 0.79 | 42 |
| 60 entries | 223 | 18.0 | 14.2 | 4.5 | 0.76 | 0.82 | 47 |
| 80 entries, full prompt (before) | 282, Whisper keeps the last 224 | **19.1** | 15.3 | **5.2** | 0.77 | 0.83 | 48 |
| 80 entries, fitted (now) | 221, label + newest 60 | **18.1** | 14.3 | **4.4** | 0.76 | 0.81 | 46 |

The fitted run goes through `pusmogo-bench --dict-file`, which builds a `Dictionary` the way the app does (prompt and replacements); the others pass the prompt text with `--prompt-file`. The no-dictionary run was repeated at the end of the session with the same result (17.5 %). No clip went empty in any run. The app's own self-test with an 80-entry dictionary in a scratch profile transcribed normally.

## Reading the numbers

- Up to the budget, each 20 entries cost about 0.1–0.3 WER points and 10–20 ms p50: the price users pay for their own words.
- Past the budget, the cut prompt costs a full point of WER and 0.7 points of CER on top. Fitting brings 80 entries back to the 60-entry level.
- Which entries to keep is a choice: the newest are kept because they are most likely the words the user is fixing now, and because that is what Whisper would keep anyway.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper && cd ..
B=core/target/release/pusmogo-bench
$B --prompt-file bench/prompts/dict80-label.txt bench/audio/fleurs-hu/hu.tsv   # cut by Whisper
$B --dict-file bench/prompts/dict80.txt bench/audio/fleurs-hu/hu.tsv           # fitted
```

The bench prints `prompt: N chars, M tokens` so a prompt over 224 tokens is visible.
