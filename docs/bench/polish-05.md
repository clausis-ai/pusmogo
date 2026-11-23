# polish-05: 1.7B local language models as a polish pass (round 2, part 1)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08. Question: do two larger local models, EuroLLM-1.7B-Instruct and Qwen3-1.7B, prompted to edit only, give a clear quality gain on real Hungarian dictation after Whisper and the rule cleanup, with zero meaning changes passed by the safety check, an added wait p50 of at most 0.5 s warm and a total footprint of at most 3.5 GB?

**Answer: no clear gain on real speech.** Qwen3-1.7B is the better of the two and the first model in this series that changes nothing for the worse, but on real clips it only moves "!" to "." and adds final full stops; commas and words stay where the rules put them. On the hand-written set both models add commas well (comma F1 0.26 → 0.65–0.70). The run also found and closed a hole in the safety check: it let a question become a statement.

## Candidates

| model | licence (Hugging Face card, 2026-11-08) | Hungarian | file | size | SHA-256 (checked) |
|---|---|---|---|---:|---|
| EuroLLM-1.7B-Instruct | Apache-2.0 (`utter-project/EuroLLM-1.7B-Instruct`) | one of its listed languages | `mradermacher/EuroLLM-1.7B-Instruct-GGUF` @ `2951f08`, Q4_K_M | 1045 MB | `1cade17f…7f84093` |
| Qwen3-1.7B | Apache-2.0 | listed among its 119 languages | `unsloth/Qwen3-1.7B-GGUF` @ `d7f544e`, Q4_K_M | 1107 MB | `b139949c…a181897` |

Not run: Qwen3-4B (the quality ceiling). Its Q4_K_M file (~2.5 GB) does not fit in what is left of this run's 6 GB download budget. Neither result points at a gap that only a bigger model could close, because on real clips the text was already right where the references differ by style.

## Setup

- llama.cpp 0.9.0 (Homebrew), Metal, Apple M5 Pro, 48 GB. `llama-server -c 4096 -ngl 99` with n-gram speculative decoding over the prompt (`--spec-type ngram-simple --spec-ngram-simple-size-n 3 --spec-ngram-simple-size-m 24`), temperature 0, thinking off for Qwen3. The weights are memory-mapped, which is llama.cpp's default.
- Prompt and worked examples: `scripts/polish-llm.py`, unchanged from [polish-02](polish-02.md). It allows punctuation, casing and keeping only the corrected half of a self-correction. The input is the rule-cleaned text.
- Sets: the 40-item polish set; 240 large-v3 outputs of real speech (FLEURS HU 152, short low-volume 44, short noisy 44); large-v3 outputs of the 40 polish items spoken by TTS; 8 long dictations (61–109 words, large-v3). Scored with `scripts/score-polish.py --guard`.
- Chat template: the template embedded in the EuroLLM GGUF drops the newline after `<|im_end|>` and so differs from the model's own `tokenizer_config.json`. With it, 30 of 40 answers came back empty and one real clip ran into an endless list ("kókuszdiókrémleves, kókuszdiókrémleves, …"): raw WER on the text set was 86.2 %. All EuroLLM numbers below use llama.cpp's built-in `chatml` template (`--chat-template chatml`), which matches the official one.

## Safety check fix

On real clips, EuroLLM turned questions into statements ("Ház vagy lakás?" → "Ház vagy lakás.", "Lift vagy lépcső?" → "…."), and the safety check passed them, because it compared words only. That is a meaning change. The check (`guard()` in `scripts/score-polish.py`) now also requires the candidate to keep the number of question marks. References still pass 40/40. Every result below, and the Qwen3-0.6B rescoring, uses the new check.

## Results

After the safety check (rules = what Pusmogo types today):

| set | system | WER | exact | F1 | F1(,) | fallbacks |
|---|---|---:|---:|---:|---:|---:|
| text, 40 | rules | 9.6 % | 10 | 0.69 | 0.26 | |
| text, 40 | EuroLLM-1.7B | 7.7 % | 22 | 0.83 | 0.65 | 4 |
| text, 40 | Qwen3-1.7B | 7.7 % | 26 | 0.86 | 0.70 | 5 |
| real speech, 240 | rules | 15.6 % | 69 | 0.72 | 0.77 | |
| real speech, 240 | EuroLLM-1.7B | 15.6 % | 69 | 0.72 | 0.77 | 74 |
| real speech, 240 | Qwen3-1.7B | 15.6 % | 76 | 0.73 | 0.77 | 19 |
| real speech, 240 | Qwen3-0.6B ([polish-02](polish-02.md), rescored) | 15.6 % | 78 | 0.72 | 0.77 | 39 |
| TTS, 40 | rules | 16.1 % | 21 | 0.81 | 0.70 | |
| TTS, 40 | EuroLLM-1.7B | 14.6 % | 22 | 0.82 | 0.72 | 4 |
| TTS, 40 | Qwen3-1.7B | 14.6 % | 23 | 0.83 | 0.74 | 5 |
| long, 8 | rules | 14.4 % | 0 | 0.77 | 0.77 | |
| long, 8 | EuroLLM-1.7B | 14.4 % | 0 | 0.77 | 0.77 | 7 |
| long, 8 | Qwen3-1.7B | 14.4 % | 0 | 0.77 | 0.77 | 3 |

Text set by subset (Qwen3-1.7B after the check): commas 9/15 exact (F1(,) 0.75), lists 5/5, safety 10/10 unchanged, self-corrections 2/10. The rule pass in [polish-03](polish-03.md) gets 9/10 on self-corrections. EuroLLM keeps the safety items' words but adds two wrong commas ("Kérek egy kávét, és egy pohár vizet.", "Szerinted ez jó ötlet, vagy nem?"), which the check lets through because they are punctuation.

Cost (llama-server process on its own; "after long" is the peak once the long inputs have run):

| model | added wait p50 / max, short and FLEURS | p50 / max, long | footprint after load | after long |
|---|---|---|---:|---:|
| EuroLLM-1.7B Q4_K_M | 0.05–0.08 / 0.45 s | 0.15 / 0.47 s | 508 MB | 741 MB |
| Qwen3-1.7B Q4_K_M | 0.07–0.11 / 0.62 s | 0.47 / 0.54 s | 572 MB | 1437 MB |

## Findings

1. **What Qwen3-1.7B changes on real speech is style.** Of 44 changed answers, 25 passed the check. All 25 are "!" → "." ("Hívj fel később!" → "Hívj fel később."), an added final full stop ("Reggel 7-kor."), "..." → ".", or a dropped pair of outer quotes. None changes meaning, adds a comma or fixes a word. The higher exact count (69 → 76) comes from references that end in "."; Qwen3-0.6B scores 78 the same way. Whether Whisper's "!" on short commands should become "." is a rule question, not a model question.
2. **Commas in real dictation are not where the room is.** As in [polish-02](polish-02.md) and [polish-04](polish-04.md), both models add commas to text without pauses (text set comma F1 0.26 → 0.65–0.70) but leave real speech at 0.77, because Whisper already puts commas at the pauses.
3. **Speed fits the bar; memory does not leave much room.** Warm added wait is under 0.12 s at p50 on short inputs. Qwen3-1.7B takes 0.47 s at p50 on the long dictations, just under the 0.5 s bar. Its footprint grows from 572 MB to 1.44 GB over long inputs (compute and KV buffers, `-c 4096`). Next to large-v3 in the app (2.05 GB peak, `stream-03`) that is about 3.5 GB with both held at once, right at the bar. The sequential plan (free Whisper's buffers, run the model, free its context) needs both engines in one process; this part does not measure it.
4. **Load time.** The server was ready about 2 s after start for both models, but each file had just been downloaded and was in the page cache, so this is a warm number. A cold load could not be measured without clearing the cache, which this run does not do.
5. **Self-corrections stay with the rule pass.** Both models fix 2 of 10 self-corrections, against 9 of 10 for the rule pass in `backtrack.rs`.

## Next

- A misheard-words set (Hungarian ASR confusions from the bench outputs, such as "López"/"lopás", "Sablás"/"rablás", "Való"/"haló", with hand-written references). This is the one job the edit-only prompt cannot do. It needs its own prompt, and a check that allows only sound-alike word swaps.
- N-best rescoring, where a small model only picks among Whisper's own beam candidates, and the fine-tune pair builder with its MLX LoRA recipe (`polish-06.md`, not started).
- The Qwen3-1.7B file stays in `~/Library/Application Support/Pusmogo/polish/` for those parts. The EuroLLM file was deleted.
