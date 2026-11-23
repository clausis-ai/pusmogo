# Context hints from the text around the cursor (context-01)

Up: [../README.md](../README.md)
Verified: 2026-11-09

Chunk C9: before a dictation the app will read the text around the cursor in the focused field; name-like words from it go into the engine prompt after the personal dictionary, so names already on screen are spelled the way they appear there. The core part is `context::hint_words`, `Dictionary::prompt_fitting_with`, the C ABI call `pusmogo_set_context` and `pusmogo-bench --context-file`; the app reads the focused field on key down (see "In the app" below).

## What counts as a hint

A word capitalised mid-sentence ("Bence"), a word with a capital after its first letter ("iPhone", "API") or one mixing letters and digits ("GPT-4"). Words capitalised only at a sentence or line start are skipped. Adjacent hint words form one hint ("Kovács Bence"); punctuation ends it. At most 16 hints, the ones nearest the cursor, from the last 2000 characters. The text itself is never kept.

The first version listed each word on its own. Whisper copied the commas: "Bencze, Lilla szerint …", "Hegyesi, Tímea". Joining full names fixed that.

## Results (large-v3-turbo q5_0, TTS, Hungarian)

Name set: 6 synthetic sentences (macOS `say -v Tünde`) with surnames, first names and product names; the context is a two-sentence note naming the same people and products.

| Run | WER % | CER % | p50 s |
|---|---|---|---|
| No context | 28.3 | 4.4 | 0.68 |
| Matching context | 20.8 | 3.6 | 0.69 |

Fixed by the context: Szűts (was Szűcs), Clausis (Klausis), Ottó (Otó), Bencze Lilla (Bence Lila → Bencze Lilas), Kubernetes casing. Still wrong: "Kertai-féle", "Gyuricza" (→ Gyuriça), "Vatson szerződést" (→ vacsonszerződést), "Rácz" (→ rád), "Supabase".

Unrelated context on FLEURS HU (100 clips; the same note, which names nothing in those clips):

| Run | WER % | CER % | p50 s |
|---|---|---|---|
| No context | 17.5 | 4.5 | 0.73 |
| Unrelated context (11 hints, 63 tokens) | 18.0 | 4.7 | 0.74 |

41 of 100 outputs changed, mostly spacing in compounds and single letters; no clip went empty. Without context the prompt is byte-identical to before (unit test), so dictation without readable context is unchanged.

## Reading

Matching context helps names clearly; unrelated context costs about half a WER point, in line with a 20-entry dictionary (dict-01). TTS is easier than real speech, and these sentences were written to contain the context's names, so the gain is an upper bound. When the app wiring lands, a field with no hint words passes nothing.

## In the app

On key down the app reads the focused field's text before the caret through Accessibility (`kAXStringForRangeParameterizedAttribute` for the last 2000 characters, else `kAXValue` cut at the caret). Password fields, secure input and apps without Accessibility give nothing. The text is handed to the core on the engine queue ahead of the session, kept across a model reload within the dictation, and cleared when the dictation ends. Only the number of hint words is logged.

Self-test with a native text field pre-filled with "Jegyzet a tegnapi hívásról: beszéltem Szűts Ottóval, Bencze Lillával és Gyuricza Péterrel a Clausis szerződésről." and a TTS clip naming the same people (4 hint words):

| Field | Output |
|---|---|
| Pre-filled | Holnap **Szűts Ottó** és **Bencze** Lila is eljön a **Clausis** megbeszélésére, Gyuriça Péter pedig online csatlakozik. |
| Context off | Holnap Szűcs Otto és Bence Lila is eljön a Klaus is megbeszélésére, Gyurica Péter pedig online csatlakozik. |
| Empty field | identical to context off |

Unrelated context on the long set (8 clips of about 60 s, the same note): streaming, which the app uses for long dictations, 18.7 → 19.4 % WER; batch 18.7 → 23.2 %, because two clips repeat their last sentence. Any prompt does this, the dictionary prompt included: a 15 s clip that says the same sentence twice comes back with it once whenever a prompt is set (also with a plain dictionary prompt and no context). Both are follow-ups for the prompt path, not for the context.

## Respelling inflected names (2026-11-07)

The prompt fixes names mostly in the forms it shows. With "Szűts Ottóval" on screen, a dictated "Ottónak" or "Bencze Lillát" often came back with the common spelling ("Otónak", "Bence Lilát"). `context::respell` now runs on the raw text before cleanup, whenever there are hints: a capitalised word that matches a hint's stem up to accents, doubled letters, "cz"/"c", "ts"/"cs" and Whisper's "ç" takes the hint's spelling and keeps its own suffix ("Szücs Otónak" → "Szűts Ottónak"). The hint may carry at most 4 suffix letters, the output word at most 6; a sentence's first word needs a 5-letter stem.

New name set: 8 TTS sentences with inflected forms of the note's names (`say -v Tünde`, not committed), same note as above.

| Run | WER % | CER % |
|---|---|---|
| No context | 29.8 | 5.1 |
| Context, prompt only | 12.8 | 2.5 |
| Context, prompt + respell | 0.0 | 0.0 |

The rules were tuned on this set, so 0.0 is an upper bound. False-positive check on FLEURS HU (100 clips), raw output identical, so any change comes from the respelling: with the unrelated note 0 of 100 outputs changed; with an adversarial note naming 16 common Hungarian surnames (Nagy, Tóth, Szabó, Kiss, Fekete …) also 0 of 100 changed. In the app (self-test with the note in a native field) "Holnap írok Szűts Ottónak a szerződésről." and "Gyuricza Péternek elküldtem a számlát." came out exact.

## English (2026-11-07)

Name set: 8 TTS sentences (`say -v "Eddy (English (US))"`, not committed) with Irish, Polish, Nigerian and Hungarian names, product names and possessives ("Ottó's", "Lilla's"); the context is an English note naming the same people and products. Before this change two hints were wrong: "I'm" (a capital mid-sentence) and "Gyuricza Péter's" (the possessive kept, so the prompt showed only that form and the respelling skipped "Otto's", which is not all letters). `context::without_clitic` now cuts "'s", "'m", "'ll", "'ve", "'d" and "'re" from hint words and from output words before respelling, then puts the output's clitic back ("Otto's" → "Ottó's"); "O'Brien" is untouched.

| Run | WER % | CER % |
|---|---|---|
| No context | 32.9 | 11.8 |
| Context, before this change | 14.3 | 4.1 |
| Context, clitics cut | 11.4 | 3.6 |

Fixed by the context: Clausis (was Klausus, Claus's), Adeyemi (at AME), Bencze Lilla (Ben Chalila), Gyuricza Péter (Jayarisha Pater), Saoirse and Vatson, Supabase (Superbase → Supabease, still off), PostgreSQL (Post Crestal), Lilla's (Lillus). Still wrong: "Kowalczyk" → "Koalczyk", "Szűts" → "Szts", "Nkechi" → "Gachi", "Aoife" → "often". The prompt still opens with the Hungarian "Gyakori szavak:" in English dictation.

False-positive checks, old against new binary: the 20-clip English synth set with an unrelated English note (9 hints) gives 0 of 20 outputs changed and 0.0 % WER either way (against no context, one comma differs); FLEURS HU with the Hungarian note gives 0 of 100 changed (the note's hints are the same). In the app (self-test, `-language en`, the note in a native field): "12 hint words", "I forwarded Ottó's notes to Benczh Lilla this morning." against "… Otto's notes to Ben Chalila …" with context off.

## A surname opening a sentence, and a dropped letter (2026-11-07)

"Szűts" stayed wrong in the English name set for two reasons. It was never a hint: the note reads "… about the Clausis contract. Szűts Ottó and Bencze Lilla will review it", and a capital at a sentence start was skipped, so only "Ottó" reached the prompt. Hungarian puts the surname first, so any sentence that opens with a full name lost its surname. And with the hint in place Whisper still wrote "Szts", one letter short, which the respelling did not match.

- `context::hint_words`: a capitalised sentence opener joins the hint when a name follows it ("Szűts Ottó"), unless it is a common opener (articles, determiners, pronouns, greetings: "The Clausis team", "Kedves Anna"). An opener on its own or followed by punctuation is still skipped.
- `context::respell_word`: the output may drop one letter of a hint of 5 letters or more, not among its first two, with at least two matching letters after the gap and the whole hint matched ("Szts" → "Szűts", "Sztsnek" → "Szűtsnek"; "Petra" never becomes "Péter").

Old (697d9b3) against new binary, the same notes as above:

| Set | Outputs changed | WER % old → new |
|---|---|---|
| English name set + English note | 1 of 8 ("SZTS" → "Szűts") | 10.0 → 8.6 |
| English synth + unrelated English note | 0 of 20 | 0.0 → 0.0 |
| Hungarian inflected-name set + Hungarian note | 0 of 8 | 0.0 → 0.0 |
| Hungarian synth + Hungarian note | 0 of 60 | 6.4 → 6.4 |
| FLEURS HU (100) + Hungarian note | 0 of 100 | 17.8 → 17.8 |
| FLEURS HU (100) + note naming 16 common surnames | 0 of 100 | 17.9 → 17.9 |

Still wrong in the English set: "Nkechi" → "Goethe", "Aoife" → "often", "Supabase" → "Supabease". TTS clips, not real voices.

## Do hints hurt names that are right without them? (2026-11-09)

One in-app self-test with "Kertész Zsófiával" on screen typed "Kerthéz" with hints on and "Kertész" with hints off. To see whether that happens again, 12 TTS sentences (`say -v Tünde`, not committed) name common Hungarian people in inflected forms (Kertész Zsófiával, Nagy Péter, Szabó Annának, Tóth Gábor, Balogh Ádámmal, Horváth Réka, Németh Lászlónak …). Each set ran with no context, with a note naming the same people ("matching"), and with a note naming near-miss people ("near": Kertai Zsolt, Nagyné Petra, Szabados Annamária, Balog Ádám, Horvát Rita, Némethy László …), each in forward and reversed clip order, because Whisper state carries over between clips.

| Model | No context | Matching (fwd / rev) | Near-miss (fwd / rev) |
|---|---|---|---|
| large-v3 q5_0, WER % | 7.1 | 0.0 / 0.0 | 1.2 / 1.2 |
| large-v3-turbo q5_0, WER % | 7.1 | 2.4 / 3.6 | 3.6 / 4.8 |

- "Kertész" was right in all 36 outputs per model. The "Kerthéz" from the app did not come back here.
- Fixed by hints: Tóth (was Tud/Tút), Annának (Anának), Balogh (matching note only), Horváth and "Szólj" (large-v3), Németh (turbo, where "Némethy" in the near note also helped).
- Near-miss hints never pulled a right name toward the note's spelling: "Horváth Réka" stayed Horváth next to "Horvát Rita", and Balogh stayed or became Balog only where the no-context output already said Balog.
- The one harm: turbo, reversed order, with either note, gave "Ádámal" for "Ádámmal", which the no-context runs had right. It is a dropped double letter in the inflection. It comes from the decoder, not the respelling: the same hints in forward order give "Ádámmal", and the respelling is a pure text step.
- Short clips (6 names, 0.8–1.3 s, both models): the outputs were identical with no, matching or near-miss context. Below 2 s the prompt carries no hints, and the respelling does not cover these misses: "Balog" → Balogh and "Horvát" → Horváth (a final letter is missing), "Tud" → Tóth, "Zsúfia" → Zsófia (a different vowel, not just an accent).

Reading: hints help much more than they hurt on these sets, and the near-miss note did no harm. The one harm was a turbo inflection that depends on clip order. TTS, not real voices.

## An old-spelling surname heard without its final "h" (2026-11-09)

With "Balogh Ádám" or "Horváth Réka" on screen, short clips still came out as "Balog" and "Horvát": the respelling allowed a dropped letter only with two matching letters after it. Now the final "h" of a "-gh" or "-th" hint of 5 letters or more may be missing in the output, also before a suffix ("Horvátnak" → "Horváthnak"). A sentence's first word gains the "h" only when the next word is capitalised too, so "Német Anna" becomes "Németh Anna" but "Német autót vettem." stays. A 4-letter hint ("Tóth") is left out; Whisper's "Tud" for "Tóth" is a different miss and stays.

Gate: every stored Hungarian bench output in the scratch folder (30 021 outputs, FLEURS, short, long, noise, name and test-set runs), respelt by the old and the new code under three notes:

| Note | Changed, old → new | New changes |
|---|---|---|
| 16 common surnames (Horváth, Németh, Balogh …) | 11 → 36 | 25, all now equal to the reference name ("Balogh Ádámmal", "Horváth Rékával", "Németh Anna") |
| Hungarian name note | 72 → 72 | 0 |
| Meeting note (`context-01` set) | 72 → 72 | 0 |

No FLEURS output changed. TTS speech, so a proxy for real voices.

## "Tóth" heard as "Tud": measured, left alone (2026-11-09)

Short clips of "Tóth Gábor" came out as "Tud Gábor" or "Tút Gábor". Before writing a rule, every stored Hungarian bench output (32 245 outputs) was scanned for a capitalised word that starts with "Tud", "Tút", "Tót" or "Tot":

| Word in the output | Count | Reference says "Tóth" |
|---|---|---|
| "Tud" or "Tút" alone, before a capitalised word | 11 | 11 |
| "Tóth" (already right) | 9 | 9 |
| "Tudod" | 2 | 0 |
| "Tudországba(n)" (a misheard word inside a FLEURS sentence) | 12 | 0 |

In the stored sets, a lone "Tud" before a name always meant Tóth. But all 11 hits come from two TTS sentences, and the same text is ordinary Hungarian: "Tud Gábor is jönni holnap?" ("Can Gábor come tomorrow too?"). That question is most likely exactly when Tóth Gábor is in the thread, so a hint for him is on screen. No text rule can tell the two apart, not even one that needs the full name "Tóth Gábor" in the note, because the question names the same person. The respelling today leaves "Tud Gábor is jönni holnap?", "Tud Gábor!" and "Tút Gábor szerint jó." unchanged with "Tóth Gábor" in the note, and stays that way: typing a wrong surname into a question is worse than missing one. A fix belongs in the model (hints in the prompt below 2 s, or a better model), not in the text step.
