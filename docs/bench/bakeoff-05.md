# bakeoff-05: noise guards for large-v3

Up: [../README.md](../README.md)
Verified: 2026-11-08

[bakeoff-03](bakeoff-03.md) found that large-v3 q5_0 writes text over music and room noise that turbo drops: a subtitle credit ("Feliratot készítette Amara.org közösségével") and "NAMASTE". This is the first guard needed before large-v3 can be the Hungarian default. Change in `core/crates/pusmogo-core/src/vad.rs`.

## Measured

Mean token probability of each invented segment, with a temporary per-segment trace (large-v3 q5_0):

| audio | detector | output | voiced s | confidence |
|---|---|---|---|---|
| music 4 s | on | NAMASTE | 2.25 | 0.862 |
| music 15 s | on | Feliratot készítette Amara.org közösségével | 8.40 | 0.855 |
| typing 4 s | off | Feliratot készítette Amara.org közösségével | 0.69 | 0.867 |
| chatter 4 s | off | Feliratok az Amara.org közösségétől | 2.88 | 0.904 |
| fan, typing, chatter 15 s | off | Feliratok az Amara.org közösségétől | 0.39–10.92 | 0.889–0.906 |

Both pass every existing check: the credit is not a known phrase, and "NAMASTE" over 2.25 s of voiced audio clears the 0.85 floor for known phrases over long audio.

To see what the floor may not touch, 100 clips of the known phrases really spoken ("Köszönjük.", "Köszönöm szépen.", "Köszönjük a figyelmet.", "Köszönöm.", "Namaste." in Hungarian; "Thank you.", "Thank you very much.", "Thanks for watching.", "Namaste.", "Bye." in English), at normal and half speed, clean and over fan, chatter, typing and music at 10 dB. The most voiced audio under any of them was 2.04 s (turbo, a slow "Thanks for watching." at 0.685, already dropped by the 0.75 rule for phrase openings); every other one stayed under 2.0 s, where the long-audio floor does not apply.

## Change

- **Subtitle credits** (`SUBTITLE_CREDITS`: the two Hungarian Amara.org credits and the English "Subtitles by the Amara.org community") are dropped whenever a segment is exactly one of them, at any confidence. A sentence that only mentions the site is kept.
- **Known phrase over long audio:** `MIN_PHRASE_CONFIDENCE` 0.85 → 0.90.

## Gate

Old and new release `pusmogo-bench`, same clip lists in the same order.

| model | detector | clips | outputs changed |
|---|---|---|---|
| large-v3 q5_0 | on | 158 (silence 6, noise 4, noise-long 4, short noisy 44, spoken known phrases 100) | 2: `noise-music-4s` "NAMASTE" and `noise-music-15s` credit → empty |
| large-v3 q5_0 | off | same 158 | 7: every credit and "NAMASTE" on the noise sets → empty |
| turbo q5_0 | on | same 158 | 0 |
| turbo q5_0 | off | same 158 | 0 |
| turbo q5_0 | on | 318 (short hu/low, FLEURS hu 100, synth hu/mixed/en/noisy) | 0 |

large-v3 now gives no text on any silence or noise clip with the detector on or off, like turbo. The 100 spoken known phrases keep the same output with both models.

Apple M5 Pro with Metal, macOS TTS (Tünde, Eddy) mixed with synthetic room sounds; synthetic speech is easier than a real voice.

## Limits

A real "Namaste" or "Thank you" spoken slowly over more than 2 s of voiced audio and decoded below 0.90 would now be dropped; none of the 100 spoken clips came close. The second large-v3 issue from bakeoff-03, background passes falling behind real time on long dictations, is not addressed here.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
M="$HOME/Library/Application Support/Pusmogo/models/ggml-large-v3-q5_0.bin"
target/release/pusmogo-bench --model "$M" --json new.json ../bench/audio/synth/{silence,noise,noise-long}.tsv \
  ../bench/audio/short/noisy.tsv kp.tsv
PUSMOGO_SPEECH_DETECTOR=0 target/release/pusmogo-bench --model "$M" --json off.json ...   # same lists
```

`kp.tsv`: the ten phrases above rendered with `say -r 175` and `say -r 90`, each also mixed over the four room sounds of `scripts/render-bench.py` at 10 dB the way `scripts/render-short.py` mixes its noisy set.
