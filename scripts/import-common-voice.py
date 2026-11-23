#!/usr/bin/env python3
"""Turn a Common Voice language folder into a training manifest in the bench format.

Common Voice (Scripted Speech, CC0-1.0) is downloaded by hand from the
Mozilla Data Collective. Its terms forbid re-hosting or re-sharing the dataset
and trying to identify a speaker, so the clips stay on this machine (and, for a
run, on the training machine until it is destroyed).

Kept: rows of validated.tsv with at least --min-up up votes and no down vote
(Common Voice's own "validated" also lets one down vote through), whose
sentence is not in reported.tsv and whose clip is not in test.tsv (the
Common Voice test split stays out of training). The speaker column is the
first 12 characters of the already-hashed client_id, enough to group a
speaker's clips and no more.

Each kept MP3 is converted with afconvert to 16 kHz mono 16-bit WAV.

`--source other` reads other.tsv instead: clips not yet voted on, never used
to train Whisper, so the fine-tuned model has not heard them. They feed the
polish model's correction pairs (`scripts/make-polish-pairs.py`, which drops
decodes too far from the sentence). The vote rule does not apply; sentences of
the test split are still dropped.

Usage:
  scripts/import-common-voice.py <out-dir> <cv-folder>/hu [--lang hu] [--min-up 2] [--jobs 8] [--source other]
"""

import argparse
import csv
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def rows(path: Path) -> list[dict]:
    if not path.exists():
        return []
    with open(path, encoding="utf-8", newline="") as f:
        return list(csv.DictReader(f, delimiter="\t", quoting=csv.QUOTE_NONE))


def convert(job: tuple[Path, Path]) -> bool:
    src, dst = job
    if dst.exists():
        return True
    tmp = dst.with_suffix(".part.wav")
    r = subprocess.run(["afconvert", "-f", "WAVE", "-d", "LEI16@16000", "-c", "1", str(src), str(tmp)],
                       capture_output=True)
    if r.returncode != 0:
        tmp.unlink(missing_ok=True)
        return False
    tmp.rename(dst)
    return True


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("out", type=Path)
    p.add_argument("cv", type=Path)
    p.add_argument("--lang", default="hu")
    p.add_argument("--min-up", type=int, default=2)
    p.add_argument("--jobs", type=int, default=8)
    p.add_argument("--source", choices=["validated", "other"], default="validated")
    a = p.parse_args()
    out = a.out.resolve()
    if out == ROOT or ROOT in out.parents:
        p.error("write the clips outside the repository (recordings of real people)")
    out.mkdir(parents=True, exist_ok=True)

    test_rows = rows(a.cv / "test.tsv")
    test = {r["path"] for r in test_rows}
    test_sentences = {" ".join(r["sentence"].split()) for r in test_rows}
    reported = {r["sentence_id"] for r in rows(a.cv / "reported.tsv") if r.get("sentence_id")}
    kept, counts = [], {a.source: 0, "votes": 0, "reported": 0, "test_split": 0, "empty": 0}
    for r in rows(a.cv / f"{a.source}.tsv"):
        counts[a.source] += 1
        if a.source == "validated" and (int(r["up_votes"] or 0) < a.min_up or int(r["down_votes"] or 0) > 0):
            counts["votes"] += 1
        elif r.get("sentence_id") in reported:
            counts["reported"] += 1
        elif r["path"] in test or (a.source == "other" and " ".join(r["sentence"].split()) in test_sentences):
            counts["test_split"] += 1
        elif not r["sentence"].strip():
            counts["empty"] += 1
        else:
            kept.append(r)

    jobs = [(a.cv / "clips" / r["path"], out / (Path(r["path"]).stem + ".wav")) for r in kept]
    ok = []
    with ThreadPoolExecutor(max_workers=max(1, a.jobs)) as pool:
        for i, done in enumerate(pool.map(convert, jobs), 1):
            ok.append(done)
            if i % 2000 == 0 or i == len(jobs):
                print(f"converted {i}/{len(jobs)}", file=sys.stderr)

    lines = []
    for r, done in zip(kept, ok):
        if done:
            text = " ".join(r["sentence"].split())
            lines.append(f"{Path(r['path']).stem}.wav\t{a.lang}\t{text}\t{r['client_id'][:12]}")
    counts["kept"] = len(lines)
    counts["convert_failed"] = len(kept) - len(lines)
    (out / f"{a.lang}.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(counts, file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
