# prompt-02: a Hungarian carrier sentence in the engine prompt

Up: [../README.md](../README.md)
Verified: 2026-11-08

[prompt-01](prompt-01.md) found that English words in the prompt (context hints from a terminal or a chat, or English dictionary names) make short Hungarian dictations come out as English respellings ("Szia" → "See ya", "Lámpa" → "Lampa"). The prompt was "Gyakori szavak: <words>." This report tests putting one short Hungarian sentence before the label, so the prompt reads as Hungarian text: **"Ez egy magyar nyelvű jegyzet. Gyakori szavak: <words>."** The English label ("Common words:") is unchanged.

## Result

| prompt | short all, before | short all, after | exact (of 132) | empty | FLEURS before → after |
|---|---|---|---|---|---|
| none (default, unchanged) | 35.1 | 35.1 | 69 → 69 | 4 → 4 | 17.3 → 17.3 |
| context: a terminal | 41.3 | 36.2 | 58 → 63 | 4 → 7 | 17.0 → 17.0 |
| context: Spanish names | 39.1 | 38.0 | 61 → 63 | 7 → 11 | 17.4 → 17.5 |
| context: an English chat message | 40.6 | 37.3 | 58 → 64 | 5 → 4 | 17.6 → 17.2 |
| dictionary: 15 English product names | 40.6 | 36.6 | 58 → 64 | 6 → 7 | 17.3 → 17.2 |

WER %, short = `bench/audio/short/{hu,low,noisy}.tsv` (132 clips, 0.6–2.6 s), large-v3-turbo q5_0, beam search, Silero speech detector on, Apple M5 Pro, `pusmogo-bench`. Synthetic speech (macOS TTS) is easier than a real voice. The context and dictionary files are the ones listed in prompt-01. "After" includes the echo guard below.

Name hints, old vs new binary, same notes as [context-01](context-01.md):

| set | before | after | changed clips |
|---|---|---|---|
| Hungarian names in a note (8 clips) | 2.1 | 0.0 | 1 ("Benczel Lilla" → "Bencze Lilla") |
| synthetic HU with the same note (60) | 7.3 | 7.5 | 8, both ways (two numbers now written as words: "4 órakor" → "négy órakor") |
| FLEURS HU with the same note (100) | 17.6 | 17.2 | 35 |
| FLEURS HU with a common-surname note (100) | 17.3 | 17.1 | 37 |

## Findings

1. **The carrier sentence wins back most of what English hints cost.** Short-clip WER under English hints falls from 40.6–41.3 % to 36.2–37.3 %, against 35.1 % with no prompt. Fixed clips are the prompt-01 kind: "Rentben" → "Rendben", "Lampa" → "Lámpa", "Roszahang" → "Rossz a hang", "See ya!" → "Szia!", "He's a fack issue." → "Hígy fagyiség." (still wrong, but no longer English).
2. **Full sentences and names do not lose.** FLEURS stays within 17.0–17.5 % under every condition, and the name sets keep their gains.
3. **New failure: the engine sometimes echoes the label.** On short noisy clips with a prompt, 7 of 528 outputs were pieces of the label ("Gyakor.", "Ez egy magyar nyelvű.", "Gyakor, Gyakor, Gyakor."). All 7 clips were already wrong without the carrier ("Eeeh!", "La Angkor.", "GitHub, Ares."). A guard (`vad::echoes_prompt_label`) now drops a segment made only of label words or their beginnings when it holds a piece of "Gyakori szavak" or three or more label words; it caught 5 of the 7 and changed no other clip. Two remain ("Gyakoráérs.", "Gyakor, Ares.").
4. **Other wordings were worse.** A longer carrier ("Magyarul diktálok, a szövegben előfordulhat néhány név vagy szakszó:") helped on clean clips but raised empty outputs (4 → 9) and noisy WER; "Magyar nyelvű szöveg, néhány idegen névvel." and "Ez egy magyar nyelvű jegyzet, magyar mondatokkal." scored 37.0 and 38.0 % with terminal hints.

## Still open

The goal was short-clip WER with terminal hints within 1 point of no prompt (≤ 36.1 %); this reaches 36.2 %. The next steps from prompt-01 are still open: leave context hints out of the prompt for very short clips (names then rely on `context::respell`), and drop hints that are plain English words when dictating Hungarian.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
S=bench/audio/short F=bench/audio/fleurs-hu/hu.tsv
core/target/release/pusmogo-bench --json term.json --context-file term.txt $S/hu.tsv $S/low.tsv $S/noisy.tsv $F
```

Carrier variants can be tried without a rebuild through `--prompt "<carrier> Gyakori szavak: <hint words>."`; the hint words the app would send are printed by `--context-file`.
