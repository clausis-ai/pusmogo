#!/usr/bin/env python3
"""Render the synthetic bench sets with macOS text-to-speech.

Reads bench/prompts/{hu,mixed,en}.txt (one sentence per line) and writes
16 kHz mono WAV clips plus manifests to bench/audio/synth/ (git-ignored):

  hu.tsv, mixed.tsv  Hungarian voice (Tünde); mixed = Hungarian with English terms
  en.tsv             English voice (Eddy, US)
  silence.tsv        silence and low noise, empty references (hallucination check)
  noise.tsv          room sounds with no speech: fan, key clicks, music, babble
  noise-long.tsv     the same sounds for 15 s (a key held while nobody speaks)
  noisy.tsv          the first Hungarian clips mixed with those sounds (must keep the text)

Manifest lines are `id<TAB>lang<TAB>reference`, as pusmogo-bench expects.
Synthetic speech is cleaner than real speech: treat scores as a lower bound.

Usage: scripts/render-bench.py [--force] [--out DIR]
"""

import argparse
import math
import random
import struct
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SETS = [
    ("hu", "hu", "Tünde"),
    ("mixed", "hu", "Tünde"),
    ("en", "en", "Eddy (English (US))"),
]
RATE = 16000


def render_tts(text: str, voice: str, wav: Path) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        aiff = Path(tmp) / "x.aiff"
        subprocess.run(["say", "-v", voice, "-o", str(aiff), text], check=True)
        subprocess.run(
            ["afconvert", "-f", "WAVE", "-d", "LEI16@16000", "-c", "1", str(aiff), str(wav)],
            check=True,
        )


def write_wav(path: Path, samples: list[float]) -> None:
    frames = b"".join(
        struct.pack("<h", max(-32768, min(32767, int(s * 32767)))) for s in samples
    )
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(frames)


def silence_clips() -> list[tuple[str, list[float]]]:
    """Inputs that must produce no text: digital silence, noise floors, a hum."""
    rng = random.Random(7)

    def noise(seconds: float, dbfs: float) -> list[float]:
        amp = 10 ** (dbfs / 20)
        return [rng.gauss(0, amp) for _ in range(int(seconds * RATE))]

    def hum(seconds: float, dbfs: float) -> list[float]:
        amp = 10 ** (dbfs / 20)
        return [amp * math.sin(2 * math.pi * 50 * i / RATE) for i in range(int(seconds * RATE))]

    return [
        ("silence-1s", [0.0] * RATE),
        ("silence-5s", [0.0] * (5 * RATE)),
        ("noise-60db-3s", noise(3, -60)),
        ("noise-45db-3s", noise(3, -45)),
        ("noise-35db-5s", noise(5, -35)),
        ("hum-40db-3s", hum(3, -40)),
    ]


def room_sounds(seconds: float) -> dict[str, list[float]]:
    """Non-speech sounds a dictation microphone hears, about -30 dBFS at their loudest."""
    rng = random.Random(11)
    n = int(seconds * RATE)

    # Fan: brown-ish noise (integrated white noise with a leak), steady.
    fan, y = [], 0.0
    for _ in range(n):
        y = 0.98 * y + rng.gauss(0, 1)
        fan.append(y)
    fan = scale(fan, -35)

    # Typing: 4 ms decaying clicks at irregular 90-250 ms intervals over a quiet floor.
    keys = [rng.gauss(0, 10 ** (-65 / 20)) for _ in range(n)]
    t = int(0.1 * RATE)
    while t < n:
        amp = 10 ** (rng.uniform(-28, -20) / 20)
        for k in range(int(0.004 * RATE)):
            if t + k < n:
                keys[t + k] += amp * math.exp(-k / 15) * rng.choice((-1, 1))
        t += int(rng.uniform(0.09, 0.25) * RATE)

    # Music: a chord progression with plucked envelopes, one note every 250 ms.
    notes = [220.0, 277.18, 329.63, 440.0, 196.0, 246.94, 293.66, 392.0]
    music = [0.0] * n
    step = int(0.25 * RATE)
    for j, start in enumerate(range(0, n, step)):
        f = notes[j % len(notes)]
        for i in range(min(step * 2, n - start)):
            env = math.exp(-i / (0.12 * RATE))
            music[start + i] += env * (math.sin(2 * math.pi * f * i / RATE)
                                       + 0.4 * math.sin(4 * math.pi * f * i / RATE))
    music = scale(music, -30)

    # Babble: noise shaped by a 4 Hz syllable-rate envelope, like distant chatter.
    babble = []
    for i in range(n):
        env = 0.55 + 0.45 * math.sin(2 * math.pi * 4.0 * i / RATE + math.sin(2 * math.pi * 0.7 * i / RATE))
        babble.append(env * rng.gauss(0, 1))
    babble = scale(babble, -32)

    return {"fan": fan, "keys": keys, "music": music, "babble": babble}


def scale(samples: list[float], dbfs: float) -> list[float]:
    rms = math.sqrt(sum(s * s for s in samples) / len(samples)) or 1.0
    k = 10 ** (dbfs / 20) / rms
    return [s * k for s in samples]


def read_wav(path: Path) -> list[float]:
    with wave.open(str(path), "rb") as w:
        raw = w.readframes(w.getnframes())
    return [v / 32768 for (v,) in struct.iter_unpack("<h", raw)]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", default=str(ROOT / "bench/audio/synth"))
    ap.add_argument("--force", action="store_true", help="re-render existing clips")
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    for name, lang, voice in SETS:
        prompts = ROOT / "bench/prompts" / f"{name}.txt"
        lines = [l.strip() for l in prompts.read_text(encoding="utf-8").splitlines() if l.strip()]
        rows = []
        for n, text in enumerate(lines, 1):
            clip_id = f"{name}{n:03d}"
            wav = out / f"{clip_id}.wav"
            if args.force or not wav.exists():
                render_tts(text, voice, wav)
            rows.append(f"{clip_id}\t{lang}\t{text}")
        (out / f"{name}.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")
        print(f"{name}: {len(rows)} clips ({voice})")

    rows = []
    for clip_id, samples in silence_clips():
        write_wav(out / f"{clip_id}.wav", samples)
        rows.append(f"{clip_id}\thu\t")
    (out / "silence.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")
    print(f"silence: {len(rows)} clips")

    sounds = room_sounds(4.0)
    rows = []
    for name, samples in sounds.items():
        write_wav(out / f"noise-{name}-4s.wav", samples)
        rows.append(f"noise-{name}-4s\thu\t")
    (out / "noise.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")
    print(f"noise: {len(rows)} clips")

    rows = []
    for name, samples in room_sounds(15.0).items():
        write_wav(out / f"noise-{name}-15s.wav", samples)
        rows.append(f"noise-{name}-15s\thu\t")
    (out / "noise-long.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")
    print(f"noise-long: {len(rows)} clips")

    # Speech over room sounds at roughly 10 dB speech-to-noise: the sound starts
    # 0.5 s before the voice and runs on after it, as when a key is held early.
    hu_rows = (out / "hu.tsv").read_text(encoding="utf-8").splitlines()[:5]
    rows = []
    for name, _ in sounds.items():
        for row in hu_rows:
            clip_id, lang, text = row.split("\t")
            speech = read_wav(out / f"{clip_id}.wav")
            lead = int(0.5 * RATE)
            total = lead + len(speech) + lead
            bed = room_sounds(total / RATE)[name]
            mixed = bed[:]
            for i, v in enumerate(speech):
                mixed[lead + i] += v * 10 ** (-6 / 20)
            noisy_id = f"{clip_id}-{name}"
            write_wav(out / f"{noisy_id}.wav", mixed)
            rows.append(f"{noisy_id}\t{lang}\t{text}")
    (out / "noisy.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")
    print(f"noisy: {len(rows)} clips")
    print(f"manifests in {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
