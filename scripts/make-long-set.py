#!/usr/bin/env python3
"""Build long dictations from the FLEURS Hungarian clips for the streaming bench.

Joins consecutive clips of bench/audio/fleurs-hu/hu.tsv (run scripts/fetch-fleurs.py
first) into ~45-60 s recordings with 0.6 s of silence between them, as a user
pausing between sentences would. Writes WAVs and hu.tsv to bench/audio/long/
(git-ignored). References are the joined FLEURS transcripts.

Usage: scripts/make-long-set.py [--per N] [--count N]
"""

import argparse
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "bench/audio/fleurs-hu"
OUT = ROOT / "bench/audio/long"
RATE = 16000


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--per", type=int, default=5, help="clips per recording")
    ap.add_argument("--count", type=int, default=8, help="recordings to build")
    args = ap.parse_args()

    rows = [l.split("\t", 2) for l in (SRC / "hu.tsv").read_text().splitlines() if l.strip()]
    OUT.mkdir(parents=True, exist_ok=True)
    gap = b"\x00\x00" * int(0.6 * RATE)
    lines = []
    for n in range(args.count):
        group = rows[n * args.per : (n + 1) * args.per]
        if len(group) < args.per:
            break
        frames = []
        for wav_name, _, _ in group:
            with wave.open(str(SRC / wav_name), "rb") as w:
                assert w.getframerate() == RATE and w.getnchannels() == 1 and w.getsampwidth() == 2
                frames.append(w.readframes(w.getnframes()))
        name = f"long-{n:02d}"
        with wave.open(str(OUT / f"{name}.wav"), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(RATE)
            w.writeframes(gap.join(frames))
        lines.append(f"{name}\thu\t{' '.join(ref.strip() for _, _, ref in group)}")
    (OUT / "hu.tsv").write_text("\n".join(lines) + "\n")
    print(f"wrote {len(lines)} recordings to {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
