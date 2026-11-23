#!/usr/bin/env python3
"""Fetch a real-speech bench sample from FLEURS (Hungarian dev or English test).

FLEURS (Google, CC-BY-4.0) has read speech from many speakers. This script
downloads one split once into a cache directory (Hungarian dev ~237 MB,
English test ~290 MB), then copies one recording per sentence for the first N
sentences (file order, so the sample is deterministic) into
bench/audio/fleurs-<lang>/ (git-ignored) and writes the manifest
bench/audio/fleurs-<lang>/<lang>.tsv (`id<TAB>lang<TAB>reference`).

The reference is FLEURS' raw transcription (with casing and punctuation);
pusmogo-bench normalises both sides before scoring. Never commit the audio:
these are recordings of real people.

Usage: scripts/fetch-fleurs.py [--lang hu|en] [--split train|dev|test]
                               [--count 100] [--every-recording]
                               [--cache DIR] [--out DIR]
Defaults: --lang hu uses the dev split (the fleurs-01 baseline), --lang en the
test split. --split train (Hungarian 1.7 GB download) feeds
scripts/make-finetune-data.py (add --every-recording and a large --count to
keep all 3,095 recordings of its 1,497 sentences, 9.3 h); never bench on it.
"""

import argparse
import subprocess
import tarfile
import urllib.request
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = "https://huggingface.co/datasets/google/fleurs/resolve/main/data"
LOCALES = {"hu": ("hu_hu", "dev"), "en": ("en_us", "test")}
CACHE_ROOT = Path.home() / "Library" / "Caches" / "Pusmogo-bench"


def fetch(url: str, dest: Path) -> None:
    if dest.exists() and dest.stat().st_size > 0:
        return
    dest.parent.mkdir(parents=True, exist_ok=True)
    part = dest.with_suffix(dest.suffix + ".part")
    print(f"downloading {url}")
    urllib.request.urlretrieve(url, part)
    part.rename(dest)


def pick(tsv: Path, count: int, every: bool = False) -> list[tuple[str, str]]:
    """First recording (or, with `every`, all recordings) of each of the first
    `count` sentences: (wav name, text)."""
    seen: set[str] = set()
    out: list[tuple[str, str]] = []
    for line in tsv.read_text(encoding="utf-8").splitlines():
        cols = line.split("\t")
        if len(cols) < 3 or (cols[0] in seen and not every):
            continue
        if cols[0] not in seen and len(seen) == count:
            break
        seen.add(cols[0])
        out.append((cols[1], cols[2].strip()))
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--lang", choices=sorted(LOCALES), default="hu")
    ap.add_argument("--split", choices=["train", "dev", "test"])
    ap.add_argument("--count", type=int, default=100)
    ap.add_argument("--every-recording", action="store_true",
                    help="keep every speaker's recording of a sentence (training sets), not just the first")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    locale, default_split = LOCALES[args.lang]
    split = args.split or default_split
    cache = args.cache or CACHE_ROOT / f"fleurs-{args.lang}"
    out = args.out or ROOT / "bench" / "audio" / f"fleurs-{args.lang}"

    tsv = cache / f"{split}.tsv"
    tgz = cache / f"{split}.tar.gz"
    fetch(f"{BASE}/{locale}/{split}.tsv", tsv)
    fetch(f"{BASE}/{locale}/audio/{split}.tar.gz", tgz)

    clips = pick(tsv, args.count, args.every_recording)
    wanted = {name: text for name, text in clips}
    out.mkdir(parents=True, exist_ok=True)

    with tarfile.open(tgz, "r:gz") as tar:
        for member in tar:
            name = Path(member.name).name
            if not member.isfile() or name not in wanted:
                continue
            data = tar.extractfile(member)
            assert data is not None
            raw = cache / "tmp.wav"
            raw.write_bytes(data.read())
            # FLEURS stores 32-bit float; the bench reads 16-bit PCM, 16 kHz, mono.
            dest = out / f"fleurs-{name}"
            subprocess.run(
                ["afconvert", "-f", "WAVE", "-d", "LEI16@16000", "-c", "1", str(raw), str(dest)],
                check=True,
            )
            raw.unlink()
            with wave.open(str(dest)) as w:
                if (w.getframerate(), w.getnchannels(), w.getsampwidth()) != (16000, 1, 2):
                    raise SystemExit(f"{name}: conversion did not give 16 kHz mono 16-bit")

    lines = []
    for name, text in clips:
        if not (out / f"fleurs-{name}").exists():
            raise SystemExit(f"{name} listed in the split TSV but missing from the archive")
        lines.append(f"fleurs-{Path(name).stem}.wav\t{args.lang}\t{text}")
    (out / f"{args.lang}.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {len(lines)} clips and {out / f'{args.lang}.tsv'}")


if __name__ == "__main__":
    main()
