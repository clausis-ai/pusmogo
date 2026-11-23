# Prompts on recordings over 30 s (longprompt-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

## The problem

With any engine prompt (personal dictionary or context hints), a recording over 30 s transcribed in one call often repeated its last sentence or ended with an invented phrase ("NAMASTE", "Köszönjük a különböző képeseket."). context-01 measured the long set at 18.7 % WER without a prompt and 23.2 % with an unrelated context note. Streaming dictations were not affected, because every streaming chunk is under 30 s.

## Cause

Whisper decodes audio over 30 s one window at a time. whisper.cpp clears its text history once, before the first window, when `no_context` is set; after each window it still stores the prompt plus that window's text and feeds the tail of it to the next window. So the second window was conditioned on the first one's last sentence and decoded it again. Without a prompt the same carry-over happens but is less likely to repeat.

## Change

`session::transcribe_all` handles a whole recording: up to 30 s it is one engine call as before; longer audio is cut at pauses like a streaming `Session` (chunks of 8 to 24 s) until the rest fits one 29 s final pass, so each sample is decoded once and every call sees only the user's prompt. `pusmogo_transcribe` (the app's non-streaming path: `PUSMOGO_STREAM=0`, or after a language swap mid-dictation), `pusmogo-bench` without `--stream` and the MCP server's `transcribe_file` use it.

## Results (large-v3-turbo q5_0, long set: 8 clips of about 60 s, Hungarian FLEURS speech)

| Run | WER % | CER % | p50 s | p95 s |
|---|---|---|---|---|
| No prompt, before | 18.7 | 4.7 | 2.45 | 3.16 |
| No prompt, after | 18.4 | 4.6 | 2.42 | 3.10 |
| Unrelated context, before | 23.2 | 8.7 | 2.46 | 3.83 |
| Unrelated context, after | 18.0 | 4.6 | 2.43 | 3.12 |

With context, long-02 and long-07 no longer repeat their last sentence (48 → 32 and 25 → 12 word errors); the invented endings on long-00, long-05 and long-06 are gone. All 16 outputs changed; per clip the no-prompt run moved by at most 4 word errors either way (long-01 22 → 18, long-02 33 → 35, long-03 11 → 13).

FLEURS HU (100 clips, no prompt): 17.5 → 17.4 % WER, p50 0.73 → 0.75 s (noise). 99 outputs are byte-identical; the only clip over 30 s (30.18 s) now takes the cut path and went from 5 to 4 word errors ("csagítette" → "segítette"). No clip went empty.

## Not fixed here

A 15 s clip that says the same sentence twice still comes back with it once whenever a prompt is set. That is one window, so this change does not touch it.
