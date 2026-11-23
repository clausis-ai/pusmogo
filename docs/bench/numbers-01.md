# Dates, times and numbers in Whisper output (numbers-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

**Result: no Hungarian number formatter is needed; one English fix shipped.** Whisper large-v3-turbo (q5_0) already writes digits, dates, percentages, decimals and phone numbers the way people type them, in both languages. The only defect was an English clock time written with a dot ("3.30 p.m."); cleanup now writes "3:30 p.m.".

## What we measured

18 synthetic clips (macOS TTS: Tünde for Hungarian, Eddy for English) read through `pusmogo-bench` with the real model, then cleanup. Synthetic speech is easier than real speech; this checks formatting, not accuracy.

| Spoken (Hungarian) | Output |
|---|---|
| március tizenötödikén … délután háromkor | március 15-én … délután háromkor |
| kettőezer-huszonhat november hetedikén | 2026. november 7-én |
| fél tízkor … tíz óra harminckor | Fél tízkor … tíz óra harminckor |
| százhuszonötezer forint | 125 ezer forint |
| huszonhárom százalékkal | 23%-kal |
| harminc, egy kettő három, négy öt hat hét | 30-123-4567 |
| nyolc óra tizenöt perckor | 8 óra 15 perckor |
| két és fél millió forint | 2,5 millió forint |
| ötvenhét darab | 57 darab |
| május harmincegy, péntek | május 31. péntek |
| három egész öt tized kilogramm | 3,5 kg |
| tizenkét ember, a második emeleten | tizenkét ember, a második emeleten |

| Spoken (English) | Output |
|---|---|
| March fifteenth at three thirty p.m. | March 15th at 3.30 p.m. → **3:30 p.m.** after cleanup |
| one hundred twenty five thousand dollars | $125,000 |
| twenty three percent | 23% |
| five five five, one two three, four five six seven | 555-123-4567 |
| fifty seven orders | 57 orders |
| May thirty first, twenty twenty six | May 31st, 2026 |

Hungarian output uses the decimal comma, the ordinal full stop and the right case suffixes ("15-én", "7-én", "23%-kal"). Times spoken as words ("háromkor", "tíz óra harminckor") stay words. That is correct Hungarian, and rewriting it to "10:30-kor" would change what the user said, so cleanup leaves it alone.

## The change

`fix_clock_dot` in `core/crates/pusmogo-core/src/cleanup.rs` rewrites `H.MM` to `H:MM` only when the next word is "a.m.", "p.m.", "am" or "pm", hour 1–12 and minute 00–59. Hungarian never uses a.m./p.m., so decimals ("3.30 dollars"), Hungarian times ("10.30-kor") and dates are untouched (unit test `english_clock_time_takes_a_colon`).

Gate: all 392 distinct outputs stored from earlier bench runs (synthetic, FLEURS, long, dictionary sets) cleaned by the previous and the new `pusmogo-mcp clean_text`: one difference, the intended clip.
