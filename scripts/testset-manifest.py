#!/usr/bin/env python3
"""Turn one speaker's recordings of the reading script into pusmogo-bench manifests.

A speaker folder holds `<id>.wav` (normal voice) and, for lines marked
whisper=yes in the script, `<id>-whisper.wav`. Any missing recording is
skipped. Writes `normal.tsv` and `whisper.tsv` into the folder, in the
`id<TAB>lang<TAB>reference` format pusmogo-bench reads (audio beside the
manifest). Also reports the speaker's consent level from `consent.txt`
(see bench/testset/CONSENT.md): `release` only when the file's first word
says so, otherwise `private`. With --by-category it also writes
`normal-<category>.tsv` per script category, so pusmogo-bench reports
accuracy per category (names, numbers, tech jargon, ...). Nothing leaves the Mac.

  scripts/testset-manifest.py SPEAKER_DIR [--script bench/testset/script.tsv] [--by-category]
  pusmogo-bench --model ... SPEAKER_DIR/normal.tsv SPEAKER_DIR/whisper.tsv
"""
import argparse
import array
import wave
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def read_script(path):
    rows = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 5 or parts[3] not in ("yes", "no") or parts[1] not in ("hu", "en"):
            sys.exit(f"{path}:{n}: expected id, lang (hu|en), category, whisper (yes|no), text")
        rows.append(parts)
    ids = [r[0] for r in rows]
    if len(set(ids)) != len(ids):
        sys.exit(f"{path}: duplicate ids")
    return rows


def consent_level(speaker_dir):
    """Return (level, warning). Anything but an explicit `release` is private."""
    path = speaker_dir / "consent.txt"
    if not path.exists():
        return "private", "no consent.txt: go through bench/testset/CONSENT.md first"
    words = path.read_text(encoding="utf-8").split()
    first = words[0].lower() if words else ""
    if first == "release":
        return "release", None
    if first == "private":
        return "private", None
    return "private", f"consent.txt does not start with private or release (got {first!r})"


def take_problem(path):
    """Return (skip, note) for a recording: unreadable or under 0.3 s is skipped,
    16-bit audio touching full scale on 3+ samples is kept with a clipping note."""
    try:
        with wave.open(str(path), "rb") as w:
            rate, width, frames = w.getframerate(), w.getsampwidth(), w.getnframes()
            data = w.readframes(frames)
    except (wave.Error, EOFError, OSError) as e:
        return True, f"unreadable ({e})"
    if rate == 0 or frames / rate < 0.3:
        return True, f"too short ({frames / rate if rate else 0:.2f} s)"
    if width == 2:
        samples = array.array("h", data)
        if sum(1 for x in samples if x >= 32767 or x <= -32768) >= 3:
            return False, "clipped"
    return False, None


def usable(path):
    if not path.exists():
        return False
    skip, note = take_problem(path)
    if note:
        print(f"warning: {path.name} {note}" + ("; skipped" if skip else ""), file=sys.stderr)
    return not skip


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("speaker_dir", type=pathlib.Path)
    ap.add_argument("--script", type=pathlib.Path, default=ROOT / "bench/testset/script.tsv")
    ap.add_argument("--by-category", action="store_true",
                    help="also write normal-<category>.tsv per script category")
    a = ap.parse_args()
    rows = read_script(a.script)
    out = {"normal": [], "whisper": []}
    by_cat = {}
    for rid, lang, cat, whisper, text in rows:
        by_cat.setdefault(cat, [])
        if usable(a.speaker_dir / f"{rid}.wav"):
            out["normal"].append(f"{rid}\t{lang}\t{text}")
            by_cat[cat].append(f"{rid}\t{lang}\t{text}")
        if whisper == "yes" and usable(a.speaker_dir / f"{rid}-whisper.wav"):
            out["whisper"].append(f"{rid}-whisper\t{lang}\t{text}")
    for name, lines in out.items():
        (a.speaker_dir / f"{name}.tsv").write_text("".join(l + "\n" for l in lines), encoding="utf-8")
    if a.by_category:
        for cat, lines in by_cat.items():
            if lines:
                (a.speaker_dir / f"normal-{cat}.tsv").write_text(
                    "".join(l + "\n" for l in lines), encoding="utf-8")
        print("categories " + ", ".join(f"{c} {len(l)}" for c, l in by_cat.items()))
    expected_whisper = sum(r[3] == "yes" for r in rows)
    level, warning = consent_level(a.speaker_dir)
    if warning:
        print(f"warning: {warning}; treated as private", file=sys.stderr)
    print(f"normal {len(out['normal'])}/{len(rows)}, whisper {len(out['whisper'])}/{expected_whisper}, consent {level}")


if __name__ == "__main__":
    main()
