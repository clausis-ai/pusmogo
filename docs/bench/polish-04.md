# polish-04: a multilingual punctuation tagger as a polish pass (part 2b, b)

Up: [../README.md](../README.md)
Verified: 2026-11-08

Date: 2026-11-08. Question: [polish-01](polish-01.md) left about 0.2 comma F1 of room on real Hungarian speech, and [polish-02](polish-02.md) showed that tiny generative models do not take it. Can a token classifier, whose only job is placing punctuation and which cannot change a word, take it within the pass bar (quality up, no meaning change on `safety`, p50 added latency ≤ 0.3 s, memory ≤ 0.5 GB)?

**Answer: no.** It places the missing commas in hand-written text well and in 3–4 ms, but on what Whisper actually types it adds nothing: Whisper already puts most commas where the speaker pauses, and the tagger's extra commas are as often wrong as right. It also needs about 0.9 GB of memory. Not adopted; the EuroLLM-1.7B run planned after it (part 2b, c) is not worth its 1.1 GB either (see Next).

## Candidate

| model | licence (Hugging Face card, 2026-11-08) | Hungarian | file | size |
|---|---|---|---|---:|
| `Horizon-Labs/punctuation-restoration-small` (mmBERT-small, 140M parameters, revision `12b02a8`) | Apache-2.0, base model Apache-2.0, trained on FineWeb / FineWeb-2 | listed; the card's own held-out Hungarian F1 is 0.92 cased, 0.87 lower-case (comma 0.90 / 0.87) | `onnx/model_quantized.onnx` (int8 embeddings), SHA-256 checked against the repo's `SHA256SUMS` | 268 MB |

It predicts, after each word, one of nothing, `.` `,` `?` `-` `:`. A Hungarian huBERT-based tagger was searched for on Hugging Face (queries "punctuation hungarian", "punctuation hu", "punctuation-restoration", "punctuat"); none with a licence turned up, so this multilingual one stands in. Other multilingual taggers with Hungarian are larger (`1-800-BAD-CODE/xlm-roberta_punctuation_fullstop_truecase`, 1.1 GB, Apache-2.0) or do not list it (`kredor/punctuate-all`, `oliverguhr/fullstop-punctuation-multilang-large`).

## Setup

- `scripts/polish-tagger.py`: ONNX Runtime on the CPU (4 threads), the model's own tokenizer, 400-token windows with 50 tokens of overlap. Input is the rule-cleaned text (`clean_text`), as for every polish candidate. Three ways to merge its marks:
  - `add`: keep every mark the text has, add a comma after a bare word when the tagger predicts one;
  - `commas`: keep sentence marks, drop the text's commas and let the tagger place all of them;
  - `full`: drop every mark and use the tagger's alone (text set only).
- Scored with `scripts/score-polish.py --guard` on the 40-item text set, the TTS outputs of the same items through large-v3, and the 240 stored real-speech outputs of large-v3 and turbo (FLEURS HU 152, short low-volume 44, short noisy 44; `finetune-01` run).
- Memory: peak resident size of the Python process (`getrusage`), broken down by step.

## Results

Text set (40 items), after the safety check:

| system | subset | WER | exact | F1 | F1(,) | fallbacks |
|---|---|---:|---:|---:|---:|---:|
| rules (today) | all | 9.6 % | 10 | 0.69 | 0.26 | |
| tagger `add` | all | 9.6 % | 23 | 0.85 | 0.70 | 0 |
| tagger `add` | commas | 0.0 % | 9 of 15 | 0.91 | 0.82 | |
| tagger `add` | list | 0.0 % | 5 of 5 | 1.00 | 1.00 | |
| tagger `add` | safety | 0.0 % | 9 of 10 | 0.97 | 0.93 | |
| tagger `commas` | all | 9.6 % | 22 | 0.90 | 0.78 | 0 |
| tagger `full` | all | 9.6 % | 21 | 0.88 | 0.78 | 0 |

Real speech and TTS (the input is what Pusmogo types today):

| set | system | exact | F1 | F1(,) | fallbacks |
|---|---|---:|---:|---:|---:|
| real speech, large-v3, 240 | rules | 69 | 0.72 | 0.77 | |
| real speech, large-v3, 240 | tagger `add` | 68 | 0.71 | 0.75 | 1 (empty output) |
| real speech, large-v3, 240 | tagger `commas` | 63 | 0.69 | 0.72 | 1 (empty output) |
| FLEURS HU, large-v3, 152 | rules | 33 | 0.76 | 0.79 | |
| FLEURS HU, large-v3, 152 | tagger `add` | 32 | 0.75 | 0.77 | |
| real speech, turbo, 240 | rules | 55 | 0.65 | 0.69 | |
| real speech, turbo, 240 | tagger `add` | 54 | 0.65 | 0.68 | 0 |
| TTS, large-v3, 40 | rules | 21 | 0.81 | 0.70 | |
| TTS, large-v3, 40 | tagger `add` | 20 | 0.80 | 0.69 | 0 |
| TTS, large-v3, 40 | tagger `commas` | 15 | 0.82 | 0.70 | 0 |

WER is unchanged everywhere (the tagger cannot touch words).

Cost:

| step | added per dictation | peak memory |
|---|---:|---:|
| tagging, p50 / p95 / max (240 real-speech inputs) | 0.003 / 0.010 / 0.027 s | |
| Python + ONNX Runtime | | 48 MB |
| + tokenizer (256k-entry vocabulary) | | 477 MB |
| + model session | | 918 MB |
| + one 400-token window | | 927 MB |

## Findings

1. **Commas in text without pauses: the tagger is good.** On the hand-written set (text with no commas by construction) comma F1 goes 0.26 → 0.70 with `add`, every list item comes out right, and no safety item changes a word. One safety item gains a comma the reference does not have.
2. **Commas in dictated text: nothing to win.** Whisper already writes the commas where the speaker pauses (comma F1 0.77 with large-v3 on real speech). The tagger's additions are a mix of correct ones ("Bush szóvivője, Gordon …", "a fő helyi sör, a number 1 …") and ones the references do not have ("az udvaron maradt, és megtagadta …", "volt, 21-20"), so F1 drops slightly (0.77 → 0.75). Letting it place every comma (`commas`) is worse (0.72): it removes commas Whisper got right. Part of what is left is reference style (FLEURS references are not consistent about commas before "és"), not something a pass can learn.
3. **Speed is no issue, memory is.** 3 ms at p50 on the CPU; the 0.5 GB bar fails at about 0.9 GB, of which roughly 430 MB is the tokenizer's 256k-entry vocabulary and 440 MB the session for a 268 MB file.
4. **Where it could still help:** long hands-free dictations spoken without pauses, the case the text set imitates. Real speech of that kind is not in any set yet; the recording kit (H5) can supply it.

## Next

- No AI polish pass is proposed for v0.1: after Qwen3-0.6B, Gemma 3 270M and this tagger, the only measured win is the self-correction rule ([polish-03](polish-03.md)), which needs no model.
- EuroLLM-1.7B (part 2b, c) is not run: its Q4 file alone is ~1.1 GB, over the memory bar, and this run shows that the comma room on real speech is mostly not recoverable by any pass that only sees the text.
- If long pause-free dictations show missing commas in real use, revisit a tagger with a smaller vocabulary (memory) on that data.
- The downloaded model was deleted after the run (budget).
