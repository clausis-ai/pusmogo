# mic-01: the capture path end to end with a held key

Up: [../README.md](../README.md)
Verified: 2026-11-07

Until now every dictation test fed a WAV file into the app (`PUSMOGO_SELFTEST_WAV`). This run drove the real path instead: a held push-to-talk key, an audio input device opened by `AudioRecorder`, the streaming session fed once a second, the live caption, the key release, and insertion into the focused app.

## Setup

- Build: `./scripts/build.sh --whisper` (3928 KB), model `ggml-large-v3-turbo-q5_0.bin`, scratch profile `PUSMOGO_PROFILE=/tmp/…sqlite`.
- Key: the second push-to-talk key set to Right ⌘ (`defaults write ai.vatson.pusmogo altKey rightCommand`), held with a synthetic modifier-only `flagsChanged` event (keycode 54, flags `0x100010`) and released with `0x100`. `PUSMOGO_DEBUG_KEYS=1` logged `alt rightCommand down`, `intent start`, then `up`, `intent stop`.
- Audio: the speakers on the test Mac were muted, so an acoustic loop (speaker into built-in microphone) was not possible; the built-in microphone gave rms 0.0007 with TTS playing. Instead the app recorded from a virtual loopback input device present on the machine (chosen as the app's microphone with `micUID`), and macOS TTS (Tünde, Hungarian, 33 s) played into that device with `say -a`. `PUSMOGO_MIC_TEST=5` on that device: 8000 Hz × 2 channels, rms 0.14.
- Target: `scripts/insert-probe.swift` (native text view).

This exercises everything after the microphone itself: device selection, format conversion (here from 8 kHz stereo), streaming, the caption and insertion. It is not a test of a real voice in a real room.

## Results

| What | Result |
|---|---|
| Streaming while held | 2 chunks of ~0.8 s each during the hold |
| Live caption | Shown above the bar during the hold, newest words ("…délelőtt 10 órakor tartunk egy rövid bemutatót, utána pedig átnézzük a visszajelzéseket.") |
| After release | total 688 ms for 65.7 s of audio: stop 78 ms, transcribe 5.7 ms (rest already streamed), insert 557 ms (`pasted`) |
| Text | 71 of 72 words exact: "küldd" came out "küld"; final "." became "!"; "tíz" written "10" |
| History | Row written with lang `hu`, latency 688 ms, app `insert-probe` |

Transcript: "Szia Péter, köszönöm a tegnapi megbeszélést. A projekt jól halad, a csapat a jövő héten befejezi az első verziót. Szerdán délelőtt 10 órakor tartunk egy rövid bemutatót, utána pedig átnézzük a visszajelzéseket. Kérlek, küld el a költségvetés frissített változatát, mert csütörtökig le kell adnunk a pénzügynek. A tesztelés során három hibát találtunk, ezeket már javítottuk. Ha bármi kérdésed van, nyugodtan hívj fel. Jó hétvégét kívánok az egész családnak!"

## Caveat from the run

The key was held for 65 s instead of the planned ~40 s, longer than the 60 s lifetime of the probe window. When the key went up the probe had already quit, the terminal was frontmost, and the text was pasted there. The app reported `pasted` correctly for the window that was in front; the test sequencing was wrong. When testing insertion, keep the target window alive well past the planned release.

## Still open

- A real voice into the built-in microphone, in a room, with the key held by hand.
- The 8 kHz device format worked; a 44.1/48 kHz USB or Bluetooth microphone has only been checked with `PUSMOGO_MIC_TEST` (no transcription).
