# polish-03: a rule pass for spoken self-corrections (part 2b, a)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08. Question: [polish-02](polish-02.md) showed that tiny language models do not fix self-corrections ("Holnap háromkor, nem, négykor találkozunk." → "Holnap négykor találkozunk."). Can a narrow rule do it without touching anything else?

## The rule

`core/crates/pusmogo-core/src/backtrack.rs`, `drop_self_corrections()`, runs on text that `clean()` already produced and only drops words:

`<wrong>, <cue>[,] [mégis|inkább] <right>` → `<right>`

- Cues: one word with its own comma (*nem, bocsánat, bocs, pardon, elnézést, javítom, helyesebben, pontosabban*; English *no, sorry, rather*) or two words (*vagyis inkább, illetve inkább, akarom mondani*; *I mean, or rather*). A bare *vagyis* ("that is") and *hanem* are no cues.
- `<wrong>` and `<right>` must be the same kind: both numbers (digits or Hungarian number words with a case ending, "hármat", "nyolckor"), both days, months or times of day, or both names (capitalised, not at a sentence start).
- A word repeated before both halves goes with the wrong one: "at 3, no, at 4" → "at 4".
- A right half that now opens the sentence gets a capital.

It is not called by the app. The plan lists backtrack among the opt-in polish features, so wiring it in (for example behind a Settings switch) is a product decision. For measurement, `pusmogo-mcp`'s `clean_text` takes `"backtrack": true` and `scripts/score-polish.py --backtrack` adds it as a system.

## Results

Polish set, text input (`scripts/score-polish.py --backtrack --guard`):

| system | subset | n | WER | exact | F1 | F1(,) |
|---|---|---:|---:|---:|---:|---:|
| rules | selfcorr | 10 | 56.8 % | 0 | 0.55 | 0.10 |
| backtrack | selfcorr | 10 | 6.8 % | 9 | 0.92 | 0.50 |
| rules / backtrack | safety | 10 | 0.0 % | 10 | 1.00 | 1.00 |

Same 40 items spoken by TTS (`say -v Tünde`), outputs stored by `pusmogo-bench` (`--bench`):

| model | system | all WER | selfcorr WER | selfcorr exact | safety changed |
|---|---|---:|---:|---:|---:|
| large-v3 | rules | 16.1 % | 68.2 % | 0 | 0 |
| large-v3 | backtrack | 8.4 % | 22.7 % | 6 | 0 |
| turbo | rules | 17.2 % | 72.7 % | 0 | 0 |
| turbo | backtrack | 10.7 % | 34.1 % | 4 | 0 |

The safety check (`guard()` in the scorer) let every change through (0 fallbacks): the rule's drops are exactly the shape the check allows.

**False fires on real and synthetic speech.** All 2437 distinct engine outputs stored from earlier runs (FLEURS HU and EN, the long, short, noise, mixed and synth sets, both models and fine-tunes) were cleaned with and without the pass. It changed 10 texts, every one a spoken self-correction from the polish set. 47 outputs contain a cue word after a comma; the closest call is FLEURS "A fő helyi sör, a number one, nem egy összetett sör": number words on both sides, kept only because "nem" has no comma after it.

## Findings

1. **A rule fixes most self-corrections, at no added time and no memory.** 9 of 10 on text, 6 of 10 (large-v3) spoken; the language models in polish-02 fixed at most 1.
2. **Misses are spelling, not shape.** Spoken misses: "Keden, bocsánat, szerdán" ("Keden" for "kedden" is no day), "Slacken, bocs, e-mailben" (no shared kind; by design), and turbo's misspellings of the words around the cue. A looser day match ("ked…") would also catch "kedves"; not done.
3. **The evidence for safety is thin where it matters.** The stored corpora are read speech and almost never hold a cue phrase; real dictations with "nem," answers ("Holnap? Nem, ma sem") will. "Holnap, nem, ma sem érek rá." is dropped to "Ma sem érek rá." today: it reads as a correction, but it can be an answer. This is why it stays opt-in.

## Next

- Product decision: offer the pass behind a Settings switch ("Önjavítás felismerése" / "Catch self-corrections"), off by default, or on.
- The punctuation tagger (part 2b, b) for commas.
