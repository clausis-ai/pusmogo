# Spoken punctuation as Whisper hears it (spoken-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

**Result:** cleanup now accepts the spellings Whisper actually writes for Hungarian punctuation commands. On 12 Hungarian TTS clips full of commands, 10 come out with the right punctuation (was 6). An engine prompt that names the commands was tried first and rejected: it fixed nothing reliably and added new errors.

## Setup

Turbo q5 (`ggml-large-v3-turbo-q5_0.bin`), Metal, greedy decoding, `pusmogo-bench --json`. 12 clips rendered with `say -v Tünde` from sentences such as "A lista kettőspont alma vessző körte vessző szilva pont". TTS is a proxy for a real voice: it pronounces commands evenly, so real speakers may fare better or worse.

## Engine prompt: rejected

Three prompts were compared with no prompt on the same clips:

| prompt | effect |
|---|---|
| a sentence listing the commands as words | "kettőspont" still "2." in 2 clips; a new "Kérdője el" appended to both questions in one clip |
| example dictation with spoken commands, no punctuation | commas dropped in one clip, "pontosvessző" → "pontoz,"; "Kérdője el" again |
| just "Kettőspont, pontosvessző, felkiáltójel." | "almaveszőkörteveszőszilva", "10. jövök", "minőségpont" |

No variant was better than no prompt, so FLEURS was not run for them ([vocab-01](vocab-01.md) already showed that a fixed prompt costs general accuracy).

## What Whisper writes, and the cleanup fix

| spoken | Whisper writes | now becomes |
|---|---|---|
| kettőspont | "ketőspont", "ketős pont", or "2." ("A lista 2. Alma") | `:` |
| felkiáltójel | "felkiáltó jel" | `!` |
| kérdőjel | "kérdő jel" (accepted for symmetry) | `?` |
| pontosvessző | "pontosvesző", "pontozvesző", "pontos vessző" | `;` |

"2." is read as a colon only between a plain word and a capitalised word, never after an article or number and never when a "1." appears earlier (a numbered list). Unit tests cover the ordinal cases ("A 2. Világháború", "a verseny 2. helyezettje", "Lista 1. Alma 2. Körte").

Gate: every distinct stored bench output (420 texts: synth, FLEURS, long, numbers, command clips) re-cleaned with the old and new `pusmogo-mcp clean_text`. 10 outputs changed, all of them command clips, all as intended.

## Per clip

| clip | before | after |
|---|---|---|
| list | A lista 2. Alma, körte, szilva. | A lista: alma, körte, szilva. |
| exclamation | Ez nagyszerű felkiáltó jel. | Ez nagyszerű! |
| semicolon | Elküldtem a jelentést pontozvesző a többit holnap. | Elküldtem a jelentést; a többit holnap. |
| colon | Fontos 2. Ne felejtsd el a kulcsot. | Fontos: ne felejtsd el a kulcsot. |

Still wrong: "Három dolog kettőspont az ár vessző…" → "3 dolog, 2 pont az ár, a határidő" ("2 pont" is also a score, so it stays; the glued "árvesző" is split since a later fix, see below) and "Az időpont kettőspont kedd" → "Az idő.ketős.ket délután" (garbled in recognition). After a "2." colon, the capital Whisper gave the next word is now lowercased ("Fontos: ne", Hungarian style), except for acronyms ("EU"); a proper noun right after such a colon loses its capital too, an accepted trade-off. Re-clean gate over 593 stored outputs: only the "list" and "colon" clips changed.

## Reproduce

Render the clips with `say -v Tünde` + `afconvert -f WAVE -d LEI16@16000 -c 1`, write a manifest `id<TAB>hu<TAB>reference`, then `core/target/release/pusmogo-bench --json out.json manifest.tsv` and compare the `output` fields.

Glued comma (follow-up): Whisper sometimes glues the misspelt "vesző" to its neighbours ("az árvesző", "almaveszőkörteveszőszilva"). Cleanup now splits such a token into words and commas, but only the single-s spelling, never after a verbal prefix ("elvesző", "kiveszőben"), before "-d" ("vesződik") or in the semicolon spellings. Re-clean gate over 874 stored outputs: 5 changed, all of them the list and colon command clips ("az ár, a határidő", "alma, körte, szilva").
