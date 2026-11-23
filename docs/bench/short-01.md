# short-01: short Hungarian dictations

Up: [../README.md](../README.md)
Verified: 2026-11-08

A first test with a real voice went badly on short Hungarian phrases ("mi a lopás vs rablás" → "Mi a López versus Sablás?") while full-sentence TTS clips scored well. [mic-02](mic-02.md) ruled out the capture path. This set makes short dictations measurable, so decoding, prompt and model changes for them can be gated.

## The set

`bench/prompts/short-hu.txt`: 44 phrases, 18 single words and 26 phrases of two to four words ("X versus Y" and "X vagy Y" questions, repeated words, greetings, words starting with h, l and r). `scripts/render-short.py --app-path <compiled mic-path-check>` renders four sets into `bench/audio/short/` (git-ignored), 0.6 to 2.6 s per clip:

| set | what |
|---|---|
| hu | Tünde, default pitch and rate |
| low | Tünde lowered in pitch (`[[pbas 30]]`) and faster (`-r 210`); macOS has only one Hungarian voice, so this stands in for a second speaker |
| noisy | the hu clips over fan, babble or key clicks (one each, in turn) at about 10 dB speech-to-noise, sound starting 0.3 s before the voice |
| app | the hu clips through the app's 48 kHz capture conversion (`scripts/mic-path-check.swift`) |

Run: `core/target/release/pusmogo-bench bench/audio/short/{hu,low,noisy,app}.tsv`. **Synthetic speech is easier than a real voice**: these numbers are a lower bound on the errors a person sees.

## Baseline (large-v3-turbo q5_0, default decoding, Apple M5 Pro)

| set | clips | WER % | WER joined % | CER % | p50 s |
|---|---|---|---|---|---|
| hu | 44 | 30.4 | 28.3 | 9.1 | 0.59 |
| low | 44 | 28.3 | 28.3 | 8.1 | 0.59 |
| noisy | 44 | 53.3 | 51.1 | 28.9 | 0.59 |
| app | 44 | 30.4 | 28.3 | 9.3 | 0.59 |

For comparison, the 60 full Hungarian TTS sentences score 7.5 % ([synth-01](synth-01.md)) and FLEURS 17.4 % ([fleurs-01](fleurs-01.md)).

## Findings

1. **Short clips are the weak spot, and TTS shows it.** On the hu set 10 of 18 single words and 9 of 26 phrases come out wrong; full sentences in the same voice are near perfect. The real-voice failure is reproducible without a real voice.
2. **The errors match the real-voice report**: foreign-looking spellings ("Lopás" → "Lopash", "Liszt" → "Líst", "Hal" → "Hall!", "Szia" → "Szió!"), r/l confusion ("Rizs" → "Líj!"), consonant length ("Reggel" → "Regél", "Hallod" → "Halod") and words glued or split at the wrong place ("Hol a lámpa?" → "Hololámpa.", "versus rablás" → "versúsra blás", "Hajó" → "Ha jól.").
3. **Some counted errors are acceptable.** "Helló" → "Hello" is a common spelling, and "hétkor" → "7-kor" is the app's number format; both are scored as errors here and should be read as such.
4. **The capture path changes nothing** (app = hu, 30.4 %), confirming mic-02 on short clips.
5. **Noise hurts short clips badly**: 53 % WER, and 5 of 44 noisy clips come out empty ("Helló", "Lámpa", "Levél", "Hol vagy most?", "Hétfőn ráérsz?"). An empty result for real speech is worse than a wrong word; the speech detector and the hallucination guard need checking on short speech in noise.
6. **Latency is flat at 0.59 s** whatever the length, so a costlier decoding for short clips (beam search, temperature fallback) has room under the ~300 ms budget.

## Per clip

| id | lang | audio s | latency s | WER % | reference | output |
|---|---|---|---|---|---|---|
| short-hu001 | hu | 0.7 | 0.58 | 100.0 | Lopás. | Lopash |
| short-hu002 | hu | 0.7 | 0.58 | 0.0 | Rablás. | Rablás! |
| short-hu003 | hu | 0.5 | 0.58 | 0.0 | Haló. | Haló! |
| short-hu004 | hu | 0.5 | 0.58 | 100.0 | Helló. | Hello! |
| short-hu005 | hu | 0.5 | 0.59 | 100.0 | Szia. | Szió! |
| short-hu006 | hu | 0.8 | 0.59 | 0.0 | Köszönöm. | Köszönöm. |
| short-hu007 | hu | 0.5 | 0.58 | 0.0 | Igen. | Igen. |
| short-hu008 | hu | 0.7 | 0.58 | 0.0 | Rendben. | Rendben. |
| short-hu009 | hu | 0.5 | 0.58 | 0.0 | Holnap. | Holnap. |
| short-hu010 | hu | 0.6 | 0.58 | 0.0 | Lámpa. | Lámpa |
| short-hu011 | hu | 0.5 | 0.59 | 100.0 | Róka. | Ruka. |
| short-hu012 | hu | 0.6 | 0.58 | 0.0 | Levél. | Levél |
| short-hu013 | hu | 0.7 | 0.58 | 100.0 | Hétfő. | Hétvő |
| short-hu014 | hu | 0.6 | 0.59 | 100.0 | Reggel. | Regél |
| short-hu015 | hu | 0.5 | 0.59 | 200.0 | Hajó. | Ha jól. |
| short-hu016 | hu | 0.4 | 0.58 | 100.0 | Liszt. | Líst |
| short-hu017 | hu | 0.4 | 0.58 | 100.0 | Rizs. | Líj! |
| short-hu018 | hu | 0.5 | 0.58 | 100.0 | Hal. | Hall! |
| short-hu019 | hu | 2.0 | 0.60 | 40.0 | Mi a lopás versus rablás? | Mi a lopás versúsra blás? |
| short-hu020 | hu | 2.4 | 0.60 | 0.0 | Haló, haló, haló, haló. | Haló, haló, haló, haló. |
| short-hu021 | hu | 1.7 | 0.59 | 0.0 | Igen, igen, igen. | Igen, igen, igen. |
| short-hu022 | hu | 1.4 | 0.59 | 0.0 | Szia, hogy vagy? | Szia, hogy vagy? |
| short-hu023 | hu | 1.4 | 0.60 | 0.0 | Jó reggelt kívánok. | Jó reggelt kívánok! |
| short-hu024 | hu | 1.3 | 0.59 | 33.3 | Helló, itt vagyok. | Hello, itt vagyok. |
| short-hu025 | hu | 1.3 | 0.59 | 0.0 | Mi a különbség? | Mi a különbség? |
| short-hu026 | hu | 1.2 | 0.59 | 0.0 | Hívj fel később. | Hívj fel később! |
| short-hu027 | hu | 1.5 | 0.59 | 0.0 | Lehet, hogy holnap. | Lehet, hogy holnap. |
| short-hu028 | hu | 1.4 | 0.59 | 0.0 | Rendben, köszi. | Rendben, köszi. |
| short-hu029 | hu | 0.9 | 0.59 | 0.0 | Hol vagy most? | Hol vagy most? |
| short-hu030 | hu | 0.9 | 0.59 | 0.0 | Rossz a hang. | Rossz a hang. |
| short-hu031 | hu | 1.4 | 0.59 | 0.0 | Lassan, kérlek. | Lassan, kérlek! |
| short-hu032 | hu | 1.2 | 0.59 | 0.0 | Kérek egy kávét. | Kérek egy kávét. |
| short-hu033 | hu | 1.1 | 0.59 | 0.0 | Látlak holnap. | Látlak holnap! |
| short-hu034 | hu | 1.3 | 0.60 | 100.0 | Hétfőn ráérsz? | Hétura érs! |
| short-hu035 | hu | 1.0 | 0.60 | 100.0 | Tea vagy kávé? | Teo, bojkávík! |
| short-hu036 | hu | 1.3 | 0.59 | 0.0 | Ház vagy lakás? | Ház vagy lakás? |
| short-hu037 | hu | 1.2 | 0.59 | 0.0 | Lift vagy lépcső? | Lift vagy lépcső. |
| short-hu038 | hu | 0.9 | 0.59 | 0.0 | Rizs vagy hal? | Rizs vagy hal? |
| short-hu039 | hu | 2.2 | 0.60 | 66.7 | Rózsa, liliom, hóvirág. | Rúzsa, Lilium, Hóvirág. |
| short-hu040 | hu | 1.0 | 0.59 | 0.0 | Levelet írok. | Levelet írok. |
| short-hu041 | hu | 1.6 | 0.59 | 33.3 | Hallod, amit mondok? | Halod, amit mondok. |
| short-hu042 | hu | 0.9 | 0.59 | 100.0 | Rögtön jövök. | Rögtönjövök. |
| short-hu043 | hu | 0.8 | 0.59 | 100.0 | Hol a lámpa? | Hololámpa. |
| short-hu044 | hu | 1.1 | 0.59 | 50.0 | Reggel hétkor. | Regel 7-kor |
| short-low001 | hu | 0.6 | 0.58 | 100.0 | Lopás. | Lopash |
| short-low002 | hu | 0.6 | 0.59 | 100.0 | Rablás. | Rabláss! |
| short-low003 | hu | 0.4 | 0.59 | 0.0 | Haló. | Haló! |
| short-low004 | hu | 0.4 | 0.58 | 100.0 | Helló. | Hello. |
| short-low005 | hu | 0.4 | 0.58 | 100.0 | Szia. | Szió! |
| short-low006 | hu | 0.7 | 0.59 | 0.0 | Köszönöm. | Köszönöm. |
| short-low007 | hu | 0.4 | 0.59 | 0.0 | Igen. | Igen. |
| short-low008 | hu | 0.6 | 0.58 | 0.0 | Rendben. | Rendben. |
| short-low009 | hu | 0.5 | 0.58 | 0.0 | Holnap. | Holnap. |
| short-low010 | hu | 0.6 | 0.59 | 0.0 | Lámpa. | Lámpa. |
| short-low011 | hu | 0.4 | 0.58 | 0.0 | Róka. | Róka. |
| short-low012 | hu | 0.5 | 0.58 | 0.0 | Levél. | Levél. |
| short-low013 | hu | 0.6 | 0.59 | 0.0 | Hétfő. | Hétfő. |
| short-low014 | hu | 0.5 | 0.58 | 100.0 | Reggel. | Regél. |
| short-low015 | hu | 0.5 | 0.59 | 200.0 | Hajó. | Ha jól. |
| short-low016 | hu | 0.4 | 0.58 | 100.0 | Liszt. | Líst. |
| short-low017 | hu | 0.4 | 0.58 | 100.0 | Rizs. | Ríj! |
| short-low018 | hu | 0.4 | 0.58 | 100.0 | Hal. | Hall! |
| short-low019 | hu | 1.7 | 0.60 | 40.0 | Mi a lopás versus rablás? | Mi a lopás versúsra blás? |
| short-low020 | hu | 2.1 | 0.60 | 0.0 | Haló, haló, haló, haló. | Haló, haló, haló, haló. |
| short-low021 | hu | 1.5 | 0.59 | 0.0 | Igen, igen, igen. | Igen, igen, igen. |
| short-low022 | hu | 1.3 | 0.59 | 0.0 | Szia, hogy vagy? | Szia, hogy vagy? |
| short-low023 | hu | 1.2 | 0.60 | 0.0 | Jó reggelt kívánok. | Jó reggelt kívánok! |
| short-low024 | hu | 1.2 | 0.59 | 33.3 | Helló, itt vagyok. | Hello, itt vagyok. |
| short-low025 | hu | 1.1 | 0.59 | 0.0 | Mi a különbség? | Mi a különbség? |
| short-low026 | hu | 1.1 | 0.59 | 0.0 | Hívj fel később. | Hívj fel később! |
| short-low027 | hu | 1.3 | 0.59 | 0.0 | Lehet, hogy holnap. | Lehet, hogy holnap. |
| short-low028 | hu | 1.2 | 0.59 | 0.0 | Rendben, köszi. | Rendben, köszi. |
| short-low029 | hu | 0.8 | 0.59 | 0.0 | Hol vagy most? | Hol vagy most? |
| short-low030 | hu | 0.8 | 0.59 | 100.0 | Rossz a hang. | Rossahang. |
| short-low031 | hu | 1.2 | 0.59 | 0.0 | Lassan, kérlek. | Lassan, kérlek! |
| short-low032 | hu | 1.0 | 0.59 | 33.3 | Kérek egy kávét. | Kérek egy kávit. |
| short-low033 | hu | 1.0 | 0.59 | 0.0 | Látlak holnap. | Látlak holnap! |
| short-low034 | hu | 1.1 | 0.59 | 100.0 | Hétfőn ráérsz? | Hétvúra érs. |
| short-low035 | hu | 0.9 | 0.59 | 33.3 | Tea vagy kávé? | Te vagy Kávé. |
| short-low036 | hu | 1.1 | 0.59 | 0.0 | Ház vagy lakás? | Ház vagy lakás? |
| short-low037 | hu | 1.1 | 0.59 | 0.0 | Lift vagy lépcső? | Lift vagy lépcső. |
| short-low038 | hu | 0.8 | 0.59 | 0.0 | Rizs vagy hal? | Rizs vagy hal. |
| short-low039 | hu | 2.0 | 0.60 | 66.7 | Rózsa, liliom, hóvirág. | Rúzsa, Lilium, Hóvirág. |
| short-low040 | hu | 0.9 | 0.59 | 0.0 | Levelet írok. | Levelet írok. |
| short-low041 | hu | 1.4 | 0.59 | 33.3 | Hallod, amit mondok? | Halod, amit mondok? |
| short-low042 | hu | 0.8 | 0.59 | 100.0 | Rögtön jövök. | Rögtönjélök. |
| short-low043 | hu | 0.7 | 0.59 | 0.0 | Hol a lámpa? | Hol a lámpa? |
| short-low044 | hu | 0.9 | 0.59 | 50.0 | Reggel hétkor. | Regel 7-kor. |
| short-noisy001-fan | hu | 1.3 | 0.59 | 100.0 | Lopás. | Lopash |
| short-noisy002-babble | hu | 1.3 | 0.59 | 100.0 | Rablás. | Ravlás! |
| short-noisy003-keys | hu | 1.1 | 0.59 | 0.0 | Haló. | Haló! |
| short-noisy004-fan | hu | 1.1 | 0.58 | 100.0 | Helló. |  |
| short-noisy005-babble | hu | 1.1 | 0.59 | 100.0 | Szia. | Ő! |
| short-noisy006-keys | hu | 1.4 | 0.59 | 0.0 | Köszönöm. | Köszönöm. |
| short-noisy007-fan | hu | 1.1 | 0.59 | 0.0 | Igen. | Igen. |
| short-noisy008-babble | hu | 1.3 | 0.59 | 100.0 | Rendben. | 9-ben |
| short-noisy009-keys | hu | 1.1 | 0.60 | 100.0 | Holnap. | Külnök. |
| short-noisy010-fan | hu | 1.2 | 0.59 | 100.0 | Lámpa. |  |
| short-noisy011-babble | hu | 1.1 | 0.59 | 100.0 | Róka. | Ruka |
| short-noisy012-keys | hu | 1.2 | 0.58 | 100.0 | Levél. |  |
| short-noisy013-fan | hu | 1.3 | 0.59 | 100.0 | Hétfő. | Hétvő! |
| short-noisy014-babble | hu | 1.2 | 0.59 | 100.0 | Reggel. | Tiregyel |
| short-noisy015-keys | hu | 1.1 | 0.58 | 200.0 | Hajó. | Ha jó! |
| short-noisy016-fan | hu | 1.0 | 0.59 | 100.0 | Liszt. | Listo. |
| short-noisy017-babble | hu | 1.0 | 0.59 | 100.0 | Rizs. | Líges |
| short-noisy018-keys | hu | 1.1 | 0.58 | 100.0 | Hal. | Hall! |
| short-noisy019-fan | hu | 2.6 | 0.60 | 20.0 | Mi a lopás versus rablás? | Mi a lopás versús rablás? |
| short-noisy020-babble | hu | 3.0 | 0.59 | 100.0 | Haló, haló, haló, haló. | Hon, hon, hon, hon |
| short-noisy021-keys | hu | 2.3 | 0.59 | 0.0 | Igen, igen, igen. | Igen, igen, igen. |
| short-noisy022-fan | hu | 2.0 | 0.59 | 33.3 | Szia, hogy vagy? | Szíva, hogy vagy? |
| short-noisy023-babble | hu | 2.0 | 0.60 | 0.0 | Jó reggelt kívánok. | Jó reggelt kívánok! |
| short-noisy024-keys | hu | 1.9 | 0.59 | 33.3 | Helló, itt vagyok. | Hello, itt vagyok. |
| short-noisy025-fan | hu | 1.9 | 0.59 | 0.0 | Mi a különbség? | Mi a különbség? |
| short-noisy026-babble | hu | 1.8 | 0.61 | 100.0 | Hívj fel később. | Hígy fagyiség. |
| short-noisy027-keys | hu | 2.1 | 0.60 | 0.0 | Lehet, hogy holnap. | Lehet, hogy holnap. |
| short-noisy028-fan | hu | 2.0 | 0.59 | 0.0 | Rendben, köszi. | Rendben, köszi. |
| short-noisy029-babble | hu | 1.5 | 0.59 | 100.0 | Hol vagy most? |  |
| short-noisy030-keys | hu | 1.5 | 0.60 | 100.0 | Rossz a hang. | Róssor hong. |
| short-noisy031-fan | hu | 2.0 | 0.59 | 0.0 | Lassan, kérlek. | Lassan, kérlek! |
| short-noisy032-babble | hu | 1.8 | 0.59 | 33.3 | Kérek egy kávét. | Kérek, egy kádi! |
| short-noisy033-keys | hu | 1.7 | 0.60 | 0.0 | Látlak holnap. | Látlak holnap! |
| short-noisy034-fan | hu | 1.9 | 0.59 | 100.0 | Hétfőn ráérsz? |  |
| short-noisy035-babble | hu | 1.6 | 0.61 | 100.0 | Tea vagy kávé? | Tehobot kárgy! |
| short-noisy036-keys | hu | 1.9 | 0.59 | 0.0 | Ház vagy lakás? | Ház vagy lakás? |
| short-noisy037-fan | hu | 1.8 | 0.59 | 0.0 | Lift vagy lépcső? | Lift vagy lépcső |
| short-noisy038-babble | hu | 1.5 | 0.60 | 100.0 | Rizs vagy hal? | Lítva tal! |
| short-noisy039-keys | hu | 2.8 | 0.60 | 100.0 | Rózsa, liliom, hóvirág. | Rúzsa, Lilium, Kúvirág |
| short-noisy040-fan | hu | 1.6 | 0.59 | 0.0 | Levelet írok. | Levelet írok. |
| short-noisy041-babble | hu | 2.2 | 0.60 | 33.3 | Hallod, amit mondok? | Halod, amit mondok! |
| short-noisy042-keys | hu | 1.5 | 0.59 | 100.0 | Rögtön jövök. | Rögtönjük. |
| short-noisy043-fan | hu | 1.4 | 0.59 | 0.0 | Hol a lámpa? | Hol a lámpa? |
| short-noisy044-babble | hu | 1.7 | 0.60 | 100.0 | Reggel hétkor. | Kregágyúttkod |
| short-app001 | hu | 0.7 | 0.58 | 100.0 | Lopás. | Lopash |
| short-app002 | hu | 0.7 | 0.59 | 0.0 | Rablás. | Rablás! |
| short-app003 | hu | 0.5 | 0.58 | 0.0 | Haló. | Haló! |
| short-app004 | hu | 0.5 | 0.58 | 100.0 | Helló. | Hello! |
| short-app005 | hu | 0.5 | 0.58 | 100.0 | Szia. | Szió! |
| short-app006 | hu | 0.8 | 0.59 | 0.0 | Köszönöm. | Köszönöm. |
| short-app007 | hu | 0.5 | 0.58 | 0.0 | Igen. | Igen. |
| short-app008 | hu | 0.7 | 0.59 | 0.0 | Rendben. | Rendben. |
| short-app009 | hu | 0.5 | 0.58 | 0.0 | Holnap. | Holnap. |
| short-app010 | hu | 0.6 | 0.58 | 0.0 | Lámpa. | Lámpa |
| short-app011 | hu | 0.5 | 0.59 | 100.0 | Róka. | Ruka. |
| short-app012 | hu | 0.5 | 0.58 | 0.0 | Levél. | Levél |
| short-app013 | hu | 0.6 | 0.58 | 100.0 | Hétfő. | Hétvő |
| short-app014 | hu | 0.6 | 0.58 | 100.0 | Reggel. | Regél |
| short-app015 | hu | 0.5 | 0.58 | 200.0 | Hajó. | Ha jól. |
| short-app016 | hu | 0.4 | 0.59 | 100.0 | Liszt. | Líst |
| short-app017 | hu | 0.4 | 0.59 | 100.0 | Rizs. | Líj! |
| short-app018 | hu | 0.5 | 0.58 | 100.0 | Hal. | Hall! |
| short-app019 | hu | 1.9 | 0.60 | 40.0 | Mi a lopás versus rablás? | Mi a lopás versúsra blás? |
| short-app020 | hu | 2.4 | 0.60 | 0.0 | Haló, haló, haló, haló. | Haló, haló, haló, haló. |
| short-app021 | hu | 1.7 | 0.60 | 0.0 | Igen, igen, igen. | Igen, igen, igen. |
| short-app022 | hu | 1.4 | 0.59 | 0.0 | Szia, hogy vagy? | Szia, hogy vagy? |
| short-app023 | hu | 1.4 | 0.60 | 0.0 | Jó reggelt kívánok. | Jó reggelt kívánok! |
| short-app024 | hu | 1.3 | 0.59 | 33.3 | Helló, itt vagyok. | Hello, itt vagyok. |
| short-app025 | hu | 1.3 | 0.59 | 0.0 | Mi a különbség? | Mi a különbség? |
| short-app026 | hu | 1.2 | 0.59 | 0.0 | Hívj fel később. | Hívj fel később! |
| short-app027 | hu | 1.5 | 0.59 | 0.0 | Lehet, hogy holnap. | Lehet, hogy holnap. |
| short-app028 | hu | 1.4 | 0.59 | 0.0 | Rendben, köszi. | Rendben, köszi. |
| short-app029 | hu | 0.9 | 0.59 | 0.0 | Hol vagy most? | Hol vagy most? |
| short-app030 | hu | 0.9 | 0.59 | 0.0 | Rossz a hang. | Rossz a hang. |
| short-app031 | hu | 1.4 | 0.59 | 0.0 | Lassan, kérlek. | Lassan, kérlek! |
| short-app032 | hu | 1.1 | 0.59 | 0.0 | Kérek egy kávét. | Kérek egy kávét. |
| short-app033 | hu | 1.1 | 0.59 | 0.0 | Látlak holnap. | Látlak holnap! |
| short-app034 | hu | 1.2 | 0.60 | 100.0 | Hétfőn ráérsz? | Hétúra éres. |
| short-app035 | hu | 1.0 | 0.59 | 100.0 | Tea vagy kávé? | Teo, bojkávíj! |
| short-app036 | hu | 1.3 | 0.59 | 0.0 | Ház vagy lakás? | Ház vagy lakás? |
| short-app037 | hu | 1.2 | 0.59 | 0.0 | Lift vagy lépcső? | Lift vagy lépcső. |
| short-app038 | hu | 0.9 | 0.59 | 0.0 | Rizs vagy hal? | Rizs vagy hal? |
| short-app039 | hu | 2.2 | 0.60 | 66.7 | Rózsa, liliom, hóvirág. | Rúzsa, Lilium, Hóvirág. |
| short-app040 | hu | 1.0 | 0.59 | 0.0 | Levelet írok. | Levelet írok. |
| short-app041 | hu | 1.6 | 0.59 | 33.3 | Hallod, amit mondok? | Halod, amit mondok. |
| short-app042 | hu | 0.9 | 0.59 | 100.0 | Rögtön jövök. | Rögtönjövök. |
| short-app043 | hu | 0.8 | 0.59 | 100.0 | Hol a lámpa? | Hololámpa. |
| short-app044 | hu | 1.0 | 0.59 | 50.0 | Reggel hétkor. | Regel 7-kor |
