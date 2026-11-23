# Head-to-head kit: Pusmogo against another dictation app

Up: [../README.md](../README.md)
Verified: 2026-11-08

Thirty phrases, read aloud once into Pusmogo and once into any other dictation app, scored the same way. The loop that builds Pusmogo never runs or touches the other app: you dictate into it yourself and save what it typed. About 15 minutes.

- Phrases: [`bench/compare/phrases.tsv`](../../bench/compare/phrases.tsv), three subsets: `hu` (15 everyday Hungarian sentences, words starting with h, l and r, "X vagy Y" questions), `tech` (10 Hungarian sentences with English tech terms), `en` (5 English).
- Scorer: [`scripts/score-compare.py`](../../scripts/score-compare.py). Every system goes through `pusmogo-bench`, so all of them get the same cleanup and the same normaliser (case, punctuation and written numbers do not count). WER = word errors / reference words, CER the same per character.
- Recordings of your voice land in `bench/audio/compare/voice/` (git-ignored). Never commit them.

## Steps

1. **Build the bench once** (a few minutes): `cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper`. The models it replays are the ones Pusmogo downloads, in `~/Library/Application Support/Pusmogo/models/` (`ggml-large-v3-turbo-q5_0.bin`, and `ggml-large-v3-q5_0.bin` for the large-v3 row; a missing model is skipped).
2. **Pusmogo, phrases 1–15.** Settings → turn on "Utolsó 20 diktálás megőrzése" (keep the last 20 dictations). Open a scratch note, pick Hungarian (fn + ⇧), and dictate `hu01` to `hu15` in order, one dictation per phrase, at your normal pace. If one goes wrong, just dictate it again right away and note its id.
3. **Collect:** `scripts/score-compare.py collect`. It pairs each kept recording with the next phrase and prints what the app typed under each reference, so a wrong pairing is easy to spot. Recordings under 0.5 s (stray taps) are skipped. If you repeated phrase X, dictate X again and every phrase after it, then run `collect --start X`.
4. **Pusmogo, phrases 16–30**, then `collect` again. Switch to English (fn + ⇧) before `en01`. The app keeps only 20 dictations, which is why collection happens in two batches.
5. **The other app.** Dictate the same 30 phrases, in the same order and language, into a plain text editor, one phrase per line (press Return between phrases yourself). If it typed nothing for a phrase, leave that line empty. Save as plain text, for example `~/Desktop/other.txt`.
6. **Score:** `scripts/score-compare.py score --text other=~/Desktop/other.txt` (add `--show` to see every output next to its reference). It prints one table:
   - `pusmogo-app`: what Pusmogo typed while you dictated,
   - `turbo`, `large-v3`: your Pusmogo recordings replayed through each model (the same audio for both, so this row pair is the fairest model comparison),
   - `other`: the other app's text file.
7. Turn the Settings switch off again if you do not want Pusmogo to keep dictations.

The two apps hear two different takes of each phrase, so a few points either way on 30 phrases is noise. Large gaps on a subset (for example `tech`) are the signal.

## Dry run with text-to-speech

`scripts/score-compare.py tts` renders the phrases with macOS voices (Tünde, Eddy) into `bench/audio/compare/tts/`, and `scripts/score-compare.py score --audio tts` replays them. Synthetic speech is easier than a real voice; these numbers only show the kit works and where the phrases are hard. Apple M5 Pro, 2026-11-08:

| system | hu WER | hu CER | tech WER | tech CER | en WER | en CER | all WER | all CER |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| turbo | 8.0 % | 1.8 % | 18.8 % | 3.9 % | 0.0 % | 0.0 % | 10.6 % | 2.2 % |
| large-v3 | 5.7 % | 1.6 % | 20.0 % | 5.6 % | 2.5 % | 0.5 % | 10.6 % | 2.9 % |

Most tech errors are English terms spelled the Hungarian way ("regvestet", "branhet", "slaken", "nodei-es"); a Dictionary entry in the app fixes those for real use, and the replay here runs with no dictionary.
