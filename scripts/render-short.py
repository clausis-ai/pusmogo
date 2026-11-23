#!/usr/bin/env python3
"""Render the short-phrase Hungarian bench set with macOS text-to-speech.

Short dictations (single words, "X versus Y" questions, repeated words,
greetings, words starting with h, l or r) are where a real voice did worst,
so they get their own gate. Reads bench/prompts/short-hu.txt and writes
16 kHz mono WAV clips plus manifests to bench/audio/short/ (git-ignored):

  hu.tsv     Tünde at her default pitch and rate
  low.tsv    Tünde lowered in pitch and a little faster: a second speaker
             as far as one installed Hungarian voice allows
  noisy.tsv  the hu.tsv clips over room sounds (fan, babble, key clicks)
             at about 10 dB speech-to-noise, sound 0.3 s before and after
  app.tsv    with --app-path BIN: the hu.tsv clips sent through the app's
             48 kHz capture conversion (BIN is scripts/mic-path-check.swift
             compiled with `swiftc -O`)

Manifest lines are `id<TAB>lang<TAB>reference`, as pusmogo-bench expects.
Synthetic speech is cleaner than real speech: treat scores as a lower bound.

Usage: scripts/render-short.py [--force] [--out DIR] [--app-path BIN]
"""

import argparse
import importlib.util
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("render_bench", ROOT / "scripts/render-bench.py")
rb = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rb)

VOICE = "Tünde"
VARIANTS = [
    # name, extra `say` arguments, text prefix (embedded speech commands)
    ("hu", [], ""),
    ("low", ["-r", "210"], "[[pbas 30]] "),
]
NOISES = ["fan", "babble", "keys"]
SNR_DB = 10


def render(text: str, args: list[str], wav: Path) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        aiff = Path(tmp) / "x.aiff"
        subprocess.run(["say", "-v", VOICE, *args, "-o", str(aiff), text], check=True)
        subprocess.run(
            ["afconvert", "-f", "WAVE", "-d", "LEI16@16000", "-c", "1", str(aiff), str(wav)],
            check=True,
        )


def rms_db(samples: list[float]) -> float:
    import math
    voiced = [s for s in samples if abs(s) > 1e-4] or [1e-4]
    return 20 * math.log10(math.sqrt(sum(s * s for s in voiced) / len(voiced)))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", default=str(ROOT / "bench/audio/short"))
    ap.add_argument("--force", action="store_true", help="re-render existing clips")
    ap.add_argument("--app-path", help="compiled scripts/mic-path-check.swift")
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    prompts = ROOT / "bench/prompts/short-hu.txt"
    lines = [l.strip() for l in prompts.read_text(encoding="utf-8").splitlines() if l.strip()]

    for name, say_args, prefix in VARIANTS:
        rows = []
        for n, text in enumerate(lines, 1):
            clip_id = f"short-{name}{n:03d}"
            wav = out / f"{clip_id}.wav"
            if args.force or not wav.exists():
                render(prefix + text, say_args, wav)
            rows.append(f"{clip_id}\thu\t{text}")
        (out / f"{name}.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")
        print(f"{name}: {len(rows)} clips")

    lead = int(0.3 * rb.RATE)
    rows = []
    for n, text in enumerate(lines, 1):
        src = out / f"short-hu{n:03d}.wav"
        speech = rb.read_wav(src)
        noise_name = NOISES[(n - 1) % len(NOISES)]
        total = lead + len(speech) + lead
        bed = rb.room_sounds(total / rb.RATE)[noise_name]
        bed = rb.scale(bed, rms_db(speech) - SNR_DB)
        for i, v in enumerate(speech):
            bed[lead + i] += v
        peak = max(abs(v) for v in bed)
        if peak > 0.99:
            bed = [v * 0.99 / peak for v in bed]
        clip_id = f"short-noisy{n:03d}-{noise_name}"
        rb.write_wav(out / f"{clip_id}.wav", bed)
        rows.append(f"{clip_id}\thu\t{text}")
    (out / "noisy.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")
    print(f"noisy: {len(rows)} clips ({SNR_DB} dB speech-to-noise)")

    if args.app_path:
        rows = []
        for n, text in enumerate(lines, 1):
            clip_id = f"short-app{n:03d}"
            wav = out / f"{clip_id}.wav"
            if args.force or not wav.exists():
                subprocess.run(
                    [args.app_path, str(out / f"short-hu{n:03d}.wav"), "48000", "1", "4800", str(wav)],
                    check=True, stdout=subprocess.DEVNULL,
                )
            rows.append(f"{clip_id}\thu\t{text}")
        (out / "app.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")
        print(f"app: {len(rows)} clips (48 kHz capture path)")

    print(f"manifests in {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
