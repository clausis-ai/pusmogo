# Skipping Whisper on noise-only clips (noise-03)

Up: [../README.md](../README.md)
Verified: 2026-11-07

[noise-02](noise-02.md) made noise-only recordings give no text, but Whisper still ran on them: about 0.5 s for 4 s of typing, fan or chatter and 0.9 s for 15 s, because those sounds pass the energy check. This round adds a speech detector in front of Whisper.

## Change

whisper.cpp ships a Silero voice activity detector (`ggml-silero-v5.1.2.bin`, 885 KB, MIT licence, public on Hugging Face under `ggml-org/whisper-vad`). When that file sits next to the Whisper model, `WhisperEngine` loads it and runs it on the CPU before each pass; a clip in which it finds no speech segment returns no text without running Whisper. Without the file, nothing changes. `PUSMOGO_SPEECH_DETECTOR=0` turns it off for A/B runs.

The detector misses short words below about -40 dBFS ("Igen.", "Yes." at 1 % of their level found nothing) while Whisper still hears them, so the clip is first scaled to a peak of 0.5 (`vad::detector_input`, level from the loudest frames skipping the top 1 %, so one key click does not set it, gain at most 1000). With that, the same words were found at 0.1 % of their level.

## Measurements

Silero alone, normalised input: 0 speech segments on all 4 s and 15 s fan, typing and babble beds, silence and hum; at least 1 on every one of 238 speech clips (FLEURS HU 100, synth HU/mixed/EN, noisy, long) and on "Igen.", "Nem.", "Yes." at full, 1 %, 0.3 % and 0.1 % level. Music gave 1 to 2 short segments, so music still goes to Whisper and the noise-02 guard. Cost about 2 ms per second of audio.

Same release `pusmogo-bench`, detector off (`PUSMOGO_SPEECH_DETECTOR=0`) and on, turbo q5:

| set | clips | outputs changed | mean latency off → on |
|---|---|---|---|
| noise (4 s) | 4 | 0 | 0.49 → 0.17 s (music clip 0.68 s, the rest 0.01 s) |
| noise-long (15 s) | 4 | 0 | 0.92 → 0.19 s |
| silence | 6 | 0 | 0.00 → 0.00 s |
| noisy | 20 | 0 | 0.685 → 0.694 s |
| synth hu / mixed / en | 60 / 30 / 20 | 0 | +6 to +8 ms |
| FLEURS hu | 100 | 0 | 0.752 → 0.777 s (repeat run) |
| long (65 s, batch) | 8 | 0 | 2.72 → 2.82 s |

No output changed on any of the 252 clips; no clip with a reference went empty. The app self-test logged `speech detector loaded`, typed nothing for 15 s of typing and typed the hu001-keys sentence unchanged.

## Limits

TTS and synthetic noise, not a real room or a real voice. The app does not download the detector yet, so it only runs where the file was placed by hand. Speech costs about 2 ms more per second of audio.
