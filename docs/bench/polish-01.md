# polish-01: what a text polish pass would have to fix (part 1: set, scorer, baseline)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08. Question: after Whisper and Pusmogo's rule cleanup (`cleanup.rs`), what is left for a local AI polish pass to fix in Hungarian dictation, and how do we score a candidate so that a meaning change is caught before it reaches the user? This part builds the test set, the scorer and the safety check, and measures today's pipeline. Candidate models are part 2.

Pipeline the experiment aims at: Whisper → rules (`clean()`) → polish model → safety check (type the rule-cleaned text when the candidate fails it) → insert.

## Set and scorer

- `bench/polish/hu.tsv`: 40 hand-written Hungarian items, `input` (text as the engine would type it) and `reference` (what a careful writer wants):
  - `selfcorr` (10): the speaker corrects themself, keep only the correction: "Holnap háromkor, nem, négykor találkozunk." → "Holnap négykor találkozunk." Cues: *nem, vagyis inkább, bocsánat, bocs, akarom mondani, javítom, illetve inkább, mégis*.
  - `commas` (15): clause commas left out (*hogy, mert, ha, aki, amit, amikor, mielőtt, bár*, address and *igen/kérlek* commas).
  - `list` (5): list commas left out.
  - `safety` (10): already right, so any change is a regression; several use *nem, hanem*, *illetve*, *vagyis* in their ordinary meaning, plus names, a date and time, English tech words.
- `scripts/score-polish.py` scores any system: WER after lower-casing and dropping punctuation, exact matches, precision/recall/F1 of the mark after each aligned word (`, . ? ! : ;`) and F1 for commas alone. `--bench FILE.json` scores outputs stored by `pusmogo-bench` instead (the output is the polish input). `--text NAME=FILE` adds a candidate (one line per item); `--guard` scores what the safety check would let through.
- Audio for the same 40 items: `say -v Tünde` of the reference (of the input for `selfcorr`), manifest `bench/audio/polish/hu.tsv` (git-ignored; render it with the snippet below). TTS is a proxy: it pauses at every comma, which helps the engine place them.

```bash
mkdir -p bench/audio/polish
grep -v '^#' bench/polish/hu.tsv | while IFS=$'\t' read id kind inp ref; do
  [ "$kind" = selfcorr ] && t=$inp || t=$ref
  say -v Tünde -o /tmp/pl-$id.aiff "$t"
  afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/pl-$id.aiff bench/audio/polish/$id.wav
  printf '%s\thu\t%s\n' "$id" "$ref"
done > bench/audio/polish/hu.tsv
```

## Safety check (`guard()` in the scorer, to be ported to Rust)

A candidate may change punctuation and casing freely. It may never add or replace a word. It may drop a run of at most 5 words only when the run ends in a self-correction cue ("háromkor, nem," before "négykor"). Anything else falls back to the rule-cleaned text. (Tightened in [polish-02](polish-02.md): the dropped run must also hold a word before the cue and be followed by a kept word.)

Check of the check, on the text set: the references fed as a candidate pass for all 40 items (0 fallbacks, all 10 self-corrections allowed). A candidate with 4 planted meaning changes (the opening words of `sf01`, `sf02` and `sf04` replaced, "vagyis két" dropped from `sf05`) falls back on all 4. A first version that allowed any drop when a cue appeared anywhere let "vagyis két" through on `sf05`; the position rule replaced it.

Known limit: the check forbids spelling fixes, because they replace a word. Several engine errors below are one-letter spellings ("Keden", "Anának", "vízet", "pinteki"). Allowing a replacement with a small character distance is a candidate relaxation for part 2, to be measured, not assumed.

## Baseline: today's pipeline

Text set (input = hand-written engine-style text, no audio):

| system | subset | n | WER | exact | F1 | F1(,) |
|---|---|---:|---:|---:|---:|---:|
| rules | all | 40 | 9.6 % | 10 | 0.69 | 0.26 |
| rules | commas | 15 | 0.0 % | 0 | 0.60 | 0.00 |
| rules | list | 5 | 0.0 % | 0 | 0.59 | 0.00 |
| rules | safety | 10 | 0.0 % | 10 | 1.00 | 1.00 |
| rules | selfcorr | 10 | 56.8 % | 0 | 0.55 | 0.10 |

The rules change nothing on these inputs (rules = raw), as expected: they have no clause comma or self-correction logic.

Same 40 items spoken by TTS, release bench (`pusmogo-bench`, q5_0 models):

| model | subset | WER | exact | F1 | F1(,) |
|---|---|---:|---:|---:|---:|
| large-v3 | all | 16.1 % | 21 | 0.81 | 0.70 |
| large-v3 | commas | 1.8 % | 13 | 0.91 | 0.90 |
| large-v3 | list | 7.3 % | 2 | 0.92 | 0.86 |
| large-v3 | safety | 10.4 % | 6 | 0.83 | 0.86 |
| large-v3 | selfcorr | 68.2 % | 0 | 0.55 | 0.10 |
| turbo | all | 17.2 % | 22 | 0.82 | 0.72 |
| turbo | commas | 0.9 % | 14 | 0.97 | 0.95 |
| turbo | selfcorr | 72.7 % | 0 | 0.55 | 0.10 |

Real speech, stored outputs of the `finetune-01` run (FLEURS HU 100 + long 8 + short clean 44 clips, same scorer):

| model | n | WER | F1 | F1(,) |
|---|---:|---:|---:|---:|
| large-v3 | 152 | 14.6 % | 0.76 | 0.79 |
| turbo | 152 | 18.5 % | 0.70 | 0.70 |

## Findings

1. **Self-corrections are the clear gap.** Every model keeps both halves ("Holnap háromkor, nem, négykor találkozunk."), and the rules do too: 0 of 10 right, 68–73 % WER on that subset. This is the case a polish pass is most likely to win, and the safety check above can let it through without letting a dropped clause through.
2. **Clause commas are mostly placed by Whisper already when the speaker pauses**: comma F1 0.90 (large-v3) and 0.95 (turbo) on TTS, which pauses at commas. On real speech the comma F1 is 0.79 (large-v3) and 0.70 (turbo), so the room is about 0.2 F1 on real voices and smaller than the text-only set suggests (0.00 there by construction). Part 2 must report the FLEURS number, not only the text set.
3. **Spelling errors are the second largest residue** ("Keden", "Anának", "vízet", "Szúj", "slaken", "CEPi Pelline"). The current safety check would block every fix of them; whether a small-distance replacement rule is safe is a part 2 measurement.
4. Engine number style differs between models ("hétkor" vs "7-kor" from turbo on `sc09`); a polish pass must not touch number style, and the scorer counts it as a word error.

## Next (part 2)

Candidates, each with its licence and Hungarian coverage checked before download (budget: under 6 GB of downloads in total, about 1.6 GB left): a Hungarian punctuation/casing tagger built on huBERT, and EuroLLM-1.7B, Qwen3-0.6B and Gemma 3 270M with a strict edit-only prompt (llama.cpp, prompt-lookup decoding). Score each with `--guard` on the text set, the TTS outputs and FLEURS (`--bench`), plus added latency p50/p95 and peak memory. Pass bar: quality up, no meaning change on `safety`, p50 added ≤ 0.3 s, memory ≤ 0.5 GB.
