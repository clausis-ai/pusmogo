# Shape of the personal dictionary prompt (dict-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

**Result: shipped.** `Dictionary::prompt()` now writes the dictionary as one labelled sentence, `Gyakori szavak: A, B, C.`, instead of a bare comma list `A, B, C`. With a 20-entry dictionary the labelled form keeps more dictionary words right and costs less general accuracy than the bare list. An empty dictionary still gives an empty prompt, so users without entries see no change.

## Why

[vocab-01](vocab-01.md) showed that Whisper reads the prompt as the text before the clip: an 86-term comma list made it drop commas, spell out ordinals and once switch to Icelandic on FLEURS. The personal dictionary used the same comma-joined shape, and a growing dictionary (C6) would push users into that failure.

## What we measured

Turbo q5, Metal, warm engine, greedy decoding, all runs in one session. Dictionary: 20 realistic entries (product and service names, two Hungarian towns, tech jargon, Hungarian tax abbreviations). Prompt files: [`bench/prompts/dict20-*.txt`](../../bench/prompts/).

| prompt shape | FLEURS WER % | joined % | CER % | outputs changed | better / worse clips | p50 s | dictionary clips (6 TTS) |
|---|---|---|---|---|---|---|---|
| none | 17.5 | 14.3 | 4.53 | 0 | | 0.73 | 1 of 6 exact |
| list `A, B, C` (old) | 17.9 | 14.3 | 4.45 | 39 | 6 / 10 | 0.74 | OTP, ÁFA fixed |
| label `Gyakori szavak: A, B, C.` (new) | 17.7 | 14.2 | 4.49 | 42 | 9 / 13 | 0.76 | OTP, ÁFA, Pusmogo fixed |
| sentence `A szövegben ezek a nevek és szavak fordulhatnak elő: A, B és C.` | 17.7 | 14.1 | 4.48 | 41 | 9 / 12 | 0.80 | OTP, ÁFA, Pusmogo fixed; Figma lowercased |

No FLEURS clip went empty in any run, and no dictionary word leaked into an output whose reference does not contain it. The English synthetic set stays at 0.0 % WER with the labelled prompt (two outputs changed punctuation, and "stand-up" became the dictionary's "standup").

The dictionary clips are six Hungarian sentences spoken by the macOS Tünde voice, each using three or four dictionary words; synthetic speech is easier than real speech.

## Reading the numbers

- The differences on FLEURS are small (0.2 points is about 4 word errors in 1,802), so the choice rests as much on the dictionary clips as on FLEURS. The labelled form is the only one that is never worse than the bare list on either.
- Every prompt costs general accuracy: 17.5 % → 17.7 % with 20 entries. That is the price of the user's own words being spelled right, and it is paid only by users who add entries.
- One FLEURS clip whose output is already nonsense without a prompt ("Rendőr segíthetően úgyn úgy…") turns into Icelandic with both list forms; only the long sentence form avoids it, at +40 ms p50. Not worth the latency for one clip that is wrong either way.
- Remaining dictionary misses ("Jira" → "Jéra", "ChatGPT TypeScript" → "6GPT-gypest script") are TTS pronunciation of English names; the replacement half of the dictionary (`from` → `to`) is the tool for those.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper && cd ..
for v in list label sentence; do
  core/target/release/pusmogo-bench --prompt-file bench/prompts/dict20-$v.txt bench/audio/fleurs-hu/hu.tsv
done
```
