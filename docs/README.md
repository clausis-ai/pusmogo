# Pusmogo documentation

Up: [../README.md](../README.md)
Verified: 2026-11-09

## Project

- [README](../README.md): what Pusmogo is, build, model download
- [CONTRIBUTING](../CONTRIBUTING.md), [Code of Conduct](../CODE_OF_CONDUCT.md), [Security](../SECURITY.md)
- [Adding a language](adding-a-language.md): the cleanup's per-language word lists
- [Data card](data-card.md): what the Hungarian fine-tune may train on, the data audit (`scripts/data-audit.py`) and what a released model must state
- [Model card](model-card-pusmogo-hu.md): the Hugging Face page of Pusmogo HU
- [Cleanup model card](model-card-pusmogo-cleanup-hu.md): the Hugging Face page of the experimental text model (not in the app)
- [Recording kit](../bench/testset/README.md): reading script for a personal test set, normal and whispered; [consent text](../bench/testset/CONSENT.md) (draft)

## For coding agents

- [AGENTS.md](../AGENTS.md): rules, commands, where to read next (`CLAUDE.md` links to it)
- [app/AGENTS.md](../app/AGENTS.md): Swift app, design tokens, copy, screenshot checks
- [core/AGENTS.md](../core/AGENTS.md): Rust engine, cleanup, C ABI, tests
- Skills in [.agents/skills/](../.agents/skills/): [bench](../.agents/skills/bench/SKILL.md), [screenshot-ui](../.agents/skills/screenshot-ui/SKILL.md)
- Check them with `scripts/check-agent-docs.sh`

## Benchmarks

- [short-01](bench/short-01.md): 44 short Hungarian phrases in four variants; 30 % WER on single words and short phrases vs 7.5 % on full sentences, the gate for short-clip fixes
- [beam-01](bench/beam-01.md): beam search (5) for final passes under 20 s; short phrases 37.3 → 35.1 %, FLEURS 17.4 → 17.0 % WER for ~30–80 ms; near-30 s windows stay greedy (beam dropped a sentence)
- [speed-01](bench/speed-01.md): where a short pass spends its time; the 30 s encoder window is ~96 % (575 of 600 ms), beam width and q8_0 weights do not change it
- [prompt-01](bench/prompt-01.md): English words in the prompt (context hints or dictionary) raise short Hungarian WER 35.1 → 39–41 % with English-style spellings; FLEURS unchanged
- [prompt-02](bench/prompt-02.md): a Hungarian carrier sentence before the prompt word list wins back most of it (terminal hints 41.3 → 36.2 % short WER, no prompt 35.1); names and FLEURS keep their numbers; label echoes dropped by a guard
- [prompt-03](bench/prompt-03.md): context hints only go into the prompt from 2 s of voiced audio; short-clip WER with English hints 36.2 → 35.5 % (no prompt 35.1), names and FLEURS unchanged
- [bakeoff-01](bench/bakeoff-01.md): turbo q5 (shipped) vs turbo q8 vs large-v3 q5 on FLEURS; large-v3 17.4 → 14.1 % WER but +0.4 s per pass and 2× memory, turbo q5 stays the default
- [bakeoff-02](bench/bakeoff-02.md): large-v3 q5 vs turbo q5 on 132 short Hungarian phrases; 35.5 → 23.9 % WER (exact 69 → 89) for +0.13 s per pass; recommendation only
- [bakeoff-03](bench/bakeoff-03.md): large-v3 q5 on silence, noise and long streamed dictations; invents subtitle credits over music and doubles the release wait, so it needs new guards and a slower step before shipping
- [bakeoff-04](bench/bakeoff-04.md): Parakeet v3 (MLX) next to turbo; 0.04 s short passes and equal FLEURS WER, but worse short phrases and no way to force the language
- [bakeoff-05](bench/bakeoff-05.md): noise guards for large-v3: subtitle credits always dropped, known-phrase floor over long audio 0.85 → 0.90; large-v3 now silent on every noise clip, turbo outputs unchanged
- [finetune-01](bench/finetune-01.md): an MIT Hungarian turbo fine-tune vs turbo and large-v3: FLEURS 13.6 % WER (large-v3 13.4, turbo 17.0) at turbo speed and memory, but stray leading punctuation, empty one-word outputs and skipped sentences in long dictations
- [finetune-02](bench/finetune-02.md): tracing the fine-tune's defects: empty one-word outputs and stray punctuation are fixable in the decoder (end-token filter: short HU 18.5 → 15.2 % WER), skipped sentences in long dictations are not
- [polish-01](bench/polish-01.md): Hungarian polish set (self-corrections, missing commas, safety items), scorer with a safety check, and today's baseline: self-corrections 0/10 right, comma F1 on real speech 0.79 (large-v3)
- [polish-02](bench/polish-02.md): Qwen3-0.6B and Gemma 3 270M as an edit-only polish pass: 0.03–0.04 s added, but no gain on real speech; safety check tightened
- [polish-03](bench/polish-03.md): a rule pass for spoken self-corrections ("háromkor, nem, négykor" → "négykor"): 9/10 on text, large-v3 spoken selfcorr WER 68 → 23 %, 0 false fires on 2437 stored outputs; not wired into the app
- [polish-04](bench/polish-04.md): a multilingual punctuation tagger (Apache-2.0, 268 MB) as a polish pass: comma F1 0.26 → 0.70 on text without pauses, but no gain on real speech (0.77 → 0.75) and ~0.9 GB of memory; not adopted, no AI polish for v0.1
- [polish-05](bench/polish-05.md): EuroLLM-1.7B and Qwen3-1.7B as an edit-only polish pass: commas on text without pauses (F1 0.26 → 0.70), but only style changes on real speech; the safety check now keeps question marks
- [polish-06](bench/polish-06.md): Qwen3-1.7B asked to fix misheard Hungarian words (55 real large-v3 outputs): 0 of 39 fixed with either prompt; a sound-alike check passes 53/55 references and blocks the model's meaning changes, but cannot tell a fix from a new error of the same shape
- [polish-07](bench/polish-07.md): Qwen3-1.7B picking among six large-v3 decodes (N-best rescoring): the best candidate per clip would give FLEURS HU 13.4 → 11.4 % WER, but the model's pick gains at most 0.5 points for about 1.7 s of extra decoding; misheard words are rarely among the candidates. Not built
- [polish-08](bench/polish-08.md): `make-polish-pairs.py` turns bench runs over training audio into (Whisper output → reference) chat pairs for a LoRA fine-tune, with a bench-leakage guard and a cap on right-as-is pairs; FLEURS references carry their own slips; MLX LoRA recipe for Qwen3-0.6B / EuroLLM-1.7B, nothing trained
- [polish-09](bench/polish-09.md): trained local models as a cleanup pass (Qwen3 1.7B, Apertus 0.5B): more damage than repair, not adopted; spell-check rules safe but tiny; what the mistakes are
- [whisper-01](bench/whisper-01.md): whispered Hungarian baseline with a pseudo-whisper generator checked against a real whispered clip: FLEURS large-v3 13.4 → 42.3 % WER, short phrases 18.5 → 87.0 %, 20 of 44 short whispers dropped by our gates
- [whisper-02](bench/whisper-02.md): a whisper-mode audio chain (band-limit, presence boost, loudness) is rejected: more short whispers pass the gates but come out as stock phrases, turbo normal FLEURS 17.0 → 25.1 %; voicing + tilt + level spread separates whispers from speech and noise
- [whisper-03](bench/whisper-03.md): decoding levers on whispered Hungarian (beam, fallback, no-speech threshold, prompt, detector threshold, guard): none helps, beam 8 short whispers 87.0 → 118.5 % WER; the guard drops no correct whispered text; W2 closed
- [train-01](bench/train-01.md): the first Hungarian turbo fine-tune (182 h, 4.56 USD on one A100): FLEURS 12.5 % WER (large-v3 13.4), short phrases 12.0 % (18.5), whispered FLEURS 29.9 % (42.3) at turbo speed; worse on English terms in Hungarian; parliament openings over noise now dropped
- [whisper-04](bench/whisper-04.md): training data for whispered Hungarian: `make-finetune-data.py` adds paired pseudo-whispered copies (25 %), a real-whisper slot with speaker holdout and a bench-leakage guard; training plan and ship gate, nothing trained
- [testset-01](bench/testset-01.md): a recording-kit speaker folder (TTS takes, pseudo-whispered pass) through `testset-manifest.py --by-category` and `pusmogo-bench`: the chain works; synthetic per-category baseline, large-v3 11.2 % / turbo 11.8 % WER overall
- [models-01](bench/models-01.md): canary-1b-v2 (CC-BY-4.0, transcribe.cpp) next to large-v3 and turbo on Hungarian: FLEURS 12.4 % WER at 0.13 s vs large-v3 13.4 % at 1.39 s, but short phrases 31.2 vs 23.9 %, no vocabulary prompt (names lose most of the hint gain); not adopted for v0.1
- [models-02](bench/models-02.md): speech models under 2 GB of memory; Nemotron unified ASR 0.6B (8-bit, MLX) is fast (0.03 s short, 0.11 s FLEURS) and invents nothing on noise, but Hungarian is below turbo (FLEURS 18.0 %, short 48.6 %); canary 4-bit still to measure
- [numbers-01](bench/numbers-01.md): dates, times and numbers in both languages; Whisper already formats them, only English "3.30 p.m." needed a fix
- [spoken-01](bench/spoken-01.md): spoken punctuation commands as Whisper writes them; cleanup accepts the variants, an engine prompt was rejected
- [context-01](bench/context-01.md): names from the text around the cursor in the engine prompt; TTS name set 28.3 → 20.8 % WER, unrelated context on FLEURS 17.5 → 18.0 %
- [vocab-01](bench/vocab-01.md): built-in tech vocabulary prompt, mixed set 18.7 → 13.2 % but FLEURS 17.5 → 18.1 %, not shipped
- [dict-01](bench/dict-01.md): personal dictionary prompt as a labelled sentence, kept more dictionary words right than a bare comma list; FLEURS 17.5 → 17.7 % with 20 entries
- [dict-02](bench/dict-02.md): dictionaries past Whisper's 224-token prompt limit lost 1.1 WER points; the prompt is now fitted (80 entries: 19.1 → 18.1 %)
- [stream-01](bench/stream-01.md): streaming sessions, one-minute dictations finish in 0.8 s instead of 2.45 s at the same WER
- [stream-02](bench/stream-02.md): large-v3 streaming steps every 2 s instead of 1 s: worst wait after release 3.16 → 2.21 s, WER within noise
- [stream-03](bench/stream-03.md): release wait in the app on the long set: large-v3 1.95 s mean (one clip 3.56 s behind a running pass) vs turbo 0.98 s, WER 15.2 vs 19.5 %
- [stream-04](bench/stream-04.md): a release abandons a chunk pass the final pass would redo: in-app wait on the long set 2.02 → 1.64 s mean, worst 2.62 → 2.06 s, WER unchanged
- [caption-01](bench/caption-01.md): caption previews of the audio not yet in a chunk; first caption 11.2 → 4.7 s at the same text, off by default because a release can wait behind one
- [caption-02](bench/caption-02.md): why previews changed the final text on the long set: one chunk cut delayed by a preview, plus carry-over between clips; previews leave no state behind
- [caption-03](bench/caption-03.md): previews with large-v3 before turning them on by default: same typed text in the bench, first caption ~4.5 s, release and memory cost in the app
- [longprompt-01](bench/longprompt-01.md): recordings over 30 s cut at pauses so a prompt no longer repeats the last sentence; long set with context 23.2 → 18.0 % WER
- [repeat-01](bench/repeat-01.md): a prompted decode that skips a repeated sentence is decoded again without the prompt (0 retries on 388 other prompted decodes)
- [mic-01](bench/mic-01.md): the held-key capture path end to end (device, streaming, live caption, insertion) with TTS through a loopback input
- [mic-02](bench/mic-02.md): the capture path does not damage the signal (48 kHz conversion transparent on FLEURS); first ~90 ms after key down not recorded
- [insert-01](bench/insert-01.md): text insertion paths (AX direct, paste, web, no field); a clipboard manager no longer makes a failed paste lose the text
- [fleurs-01](bench/fleurs-01.md): first real-speech baseline, 100 Hungarian FLEURS clips
- [fleurs-en-01](bench/fleurs-en-01.md): 100 English FLEURS test clips, turbo vs large-v3, peak memory per model
- [compare-README](bench/compare-README.md): head-to-head kit, 30 phrases dictated into Pusmogo and another app, one WER/CER table per subset and system
- [scoring-01](bench/scoring-01.md): normaliser fixes and the compound-tolerant "WER joined" column, both sets re-scored
- [guard-01](bench/guard-01.md): hallucination guard, silence and noise give no text, accuracy unchanged
- [noise-01](bench/noise-01.md): typing, music and chatter no longer produce "Köszönöm."; speech over noise keeps its text
- [noise-02](bench/noise-02.md): 15 s of noise no longer produces "NAMASTE" or other invented words; spoken short phrases kept
- [noise-03](bench/noise-03.md): a speech detector skips Whisper on typing, fan and chatter (0.9 s → 0.03 s for 15 s), outputs unchanged
- [noise-04](bench/noise-04.md): short words in noise are typed as near misses instead of nothing (empty outputs on spoken clips 4 → 0), noise still gives no text
- [noise-05](bench/noise-05.md): "Köszönjük!" invented over typing and chatter is dropped, spoken "Köszönjük" kept (1/396 outputs changed with the detector on)
- [noise-06](bench/noise-06.md): decoder settings (temperature fallback, more fallback candidates, a noise gate) do not fix short words in noise with turbo; the model does
- [flashattn-01](bench/flashattn-01.md): flash attention on (about 5 % faster, accuracy unchanged), thread count makes no difference
- [audioctx-01](bench/audioctx-01.md): shorter encoder window for short clips, rejected (2.2x faster, +9 WER points on real speech)
- [synth-01](bench/synth-01.md): baseline on the 116-clip synthetic set (Hungarian, mixed, English, silence)
- [bench-01](bench/bench-01.md): first `pusmogo-bench` run (turbo q5, synthetic clips)
- [e2e-01](bench/e2e-01.md): first end-to-end run with the real model through the MCP server

## Development notes

