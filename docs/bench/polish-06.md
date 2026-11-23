# polish-06: a local language model fixing misheard Hungarian words (round 2, part 2a)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08. Question: the edit-only prompt of [polish-05](polish-05.md) may not change words, so it cannot touch the errors that matter most on real speech: words Whisper heard wrong ("Holnagy" for "Holnap", "csúgálja" for "sugallja"). Can Qwen3-1.7B fix those when it is allowed to swap a word for a similar-sounding one, and can a check keep it from changing anything else?

**Answer: no.** Qwen3-1.7B fixes 0 of 39 misheard items with either prompt, and the one word it changes that passes the check makes a right word wrong. The sound-alike check works as a fence (it passes 53 of 55 hand-written references and blocks both meaning changes the model made), but there is nothing behind it to let through.

## The set

`bench/polish/misheard-hu.tsv`, 55 items, all real large-v3 outputs taken unchanged from stored bench runs (no TTS-only text, no audio in the repo):

| kind | items | source | what the reference is |
|---|---:|---|---|
| short | 13 | `short-01` clean, low-volume and noisy runs | the output with only the misheard words fixed ("Holnagy." → "Holnap.", "Leveletírók" → "Levelet írok", "Te vagy kávé?" → "Tea vagy kávé?") |
| sentence | 26 | FLEURS HU | the same, 1 to 4 words per sentence ("fogalmazásgátló" → "fogamzásgátló", "hegyenként" → "helyenként", "bennmentesen" → "vámmentesen") |
| keep | 16 | both | already right and must not change, picked to tempt a "fix": "Lopás", "Haló!", "Liszt", "Rizs vagy hal?", "katalán", "curry", "argó", "George W. Bush" |

The references fix words only: punctuation and number formatting stay as typed, so the score measures word fixes alone. Errors with nothing in the audio to go on (a wrong name, "Hall" for "Hal") were left out, because no text-only pass could know.

## The sound-alike check

`soundalike_guard()` in `scripts/polish-misheard.py`. The candidate may be typed only when: question marks are kept; no digit is added, dropped or changed; each changed run is at most 3 words on either side and, accents folded and spaces removed, has a character similarity of at least 0.5 to what it replaces; and at most a third of the typed words (at least two) change. Otherwise the rule-cleaned text is typed. An added or dropped word on its own never passes.

Test of the check with the references as the candidate: 53/55 pass. The two blocked are fixes too far from the sound to allow safely: "Te óvodj kávé" → "Tea vagy kávé", and a three-word phrase where all three words change.

## Setup

llama.cpp 0.9.0 (Homebrew), Metal, Apple M5 Pro, `llama-server -c 4096 -ngl 99 --jinja` with n-gram speculation, temperature 0, thinking off. Qwen3-1.7B Q4_K_M, the hash-checked file from [polish-05](polish-05.md). Input: the rule-cleaned text (on this set the rules change no words). Two prompts, both in `scripts/polish-misheard.py`: the default (fix only misheard words, return the text unchanged when all words are right, five worked examples of which two change nothing) and `--eager` (an error is likely; only the three changing examples).

## Results

| system | short WER | sentence WER | keep changed | items fixed | items broken |
|---|---:|---:|---:|---:|---:|
| rules (today) | 81.5 % | 11.2 % | 0/16 | | |
| Qwen3-1.7B | 81.5 % | 11.4 % | 0/16 | 0/39 | 1 |
| Qwen3-1.7B, after the check | 81.5 % | 11.4 % | 0/16 | 0/39 | 1 |
| Qwen3-1.7B `--eager` | 81.5 % | 11.4 % | 1/16 | 0/39 | 2 |
| Qwen3-1.7B `--eager`, after the check | 81.5 % | 11.4 % | 0/16 | 0/39 | 1 |
| references (check test) | 0.0 % | 0.0 % | 0/16 | 39/39 | 0 |

Added wait p50 0.11 s, p95 0.19 s, max 0.32 s (warm). Server footprint 611 MB after the run.

## Findings

1. **The model does not find the errors.** With the default prompt it returned 53 of 55 items word for word. The two it changed were already wrong in other places, and the changes were new misspellings ("végirazmény" → "végirazmány", "feltűnéskeltésre" → "feltűnéskeltésra"). Telling it an error is likely made no difference to the misheard words; it only started cutting: "Lift vagy lépcső?" → "Lift.", "Halod, amit mondók?" → "Halod, amit mondsz.". Both were blocked by the check.
2. **The check lets through a sound-alike change for the worse.** "feltűnéskeltésre" → "feltűnéskeltésra" and "nacionalizmus" → "nacionalizmusra" pass, because they look like the misheard fixes the check exists to allow. A text-only check cannot tell a fix from a new error of the same shape; only the audio can. That is the case for the N-best route: let the model pick only among spellings Whisper itself considered for that audio.
3. **Short phrases are the hard case either way.** Short WER stays at 81.5 % for every system, and two of the fixes ("Te óvodj" → "Tea vagy") are too far from the typed sound for any safe text check. A one- to three-word dictation gives a language model almost no context.
4. **Next model size is not the answer on this evidence.** A 1.7B model that copies its input does not suggest that a larger one would be safe, and Qwen3-4B (~2.5 GB) does not fit in what is left of this run's download budget. The next step is N-best rescoring (part b), where the model cannot write a word Whisper did not propose.

## Reproduce

```bash
# needs core/target/release/pusmogo-mcp (cargo build --release -p pusmogo-mcp)
scripts/polish-misheard.py            # rules, raw and the check test, no model
llama-server -m Qwen3-1.7B-Q4_K_M.gguf --port 8090 -ngl 99 -c 4096 --jinja \
    --spec-type ngram-simple --spec-ngram-simple-size-n 3 --spec-ngram-simple-size-m 24
scripts/polish-misheard.py --port 8090 --name qwen3-1.7b
scripts/polish-misheard.py --port 8090 --name qwen3-1.7b-eager --eager
```
