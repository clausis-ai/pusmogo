# repeat-01: a prompt that skips a repeated sentence

Up: [../README.md](../README.md)
Verified: 2026-11-07

## The problem

With any engine prompt (personal dictionary or context hints), a 15 s clip that says the same sentence twice came back with the sentence once. It is one 30 s window, so the cut at pauses from [longprompt-01](longprompt-01.md) does not reach it. Segment timestamps still cover the whole clip (0.00–15.40 s), so a "text ends before the audio does" check cannot see it.

## Signal

The text is far too short for the voiced audio. Characters per voiced second (frame-energy voice from `vad::VoiceActivity`), large-v3-turbo q5_0:

| Clip | Chars / voiced s |
|---|---|
| Repeat clip, prompt (sentence once) | 8.0 |
| Same clip, no prompt (both sentences) | 15.9 |
| FLEURS HU, 110 segments of ≥ 3 s voice, no prompt: lowest / median | 12.1 / 17.0 |
| FLEURS HU, same, with a context note: lowest / median | 11.7 / 16.6 |

## Change

`vad::prompt_skipped_speech`: with a prompt, at least 4 s of voice and fewer than 9 characters per voiced second, the engine decodes once more without the prompt. `vad::prefer_plain` keeps that result only if it is at least 1.5 times longer; otherwise the prompted text stays, since it spells names better. A false trigger costs one extra decode, never text.

## Results

| Run | Prompted decodes | Retries | Outputs vs before |
|---|---|---|---|
| Repeat clip pair, context note | 2 | 1 | repeat clip now has both sentences; the other unchanged |
| FLEURS HU 100, context note | 101 | 0 | 100/100 byte-identical, WER 17.9 % |
| FLEURS HU 100, 80-entry dictionary | 101 | 0 | unchanged path |
| Long set 8 × ~60 s, streaming, context | 50 | 0 | unchanged path |
| Long set, batch, context | 26 | 0 | unchanged path |
| Synth HU + mixed + EN 110, context | 110 | 0 | unchanged path |

When nothing triggers, the code runs exactly the old single decode. The app's self-test of the repeat clip with four context hints from the focused field now types both sentences.

Cost: the recovered repeat clip loses the prompt's spelling ("Szűcs Otó" instead of "Szűts Ottó"). Getting both the repeat and the names would need a second prompted pass over the uncovered half; not worth it for how rarely people dictate a sentence twice.

Audio is macOS TTS (Tünde), a proxy for real speech. Slow, drawn-out real speech could sit nearer the 9 chars/s line; then it only costs one extra decode.
