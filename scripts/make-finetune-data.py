#!/usr/bin/env python3
"""Build a Hungarian speech fine-tune set with pseudo-whispered copies.

Whisper (the model) types nothing useful for whispered Hungarian, and decoding
or audio-path changes do not fix it (docs/bench/whisper-01..03.md): the model
has to hear whispers in training. Real whispered recordings are scarce, so
this script mixes three kinds of clip into one training set:

  normal          the source clip as recorded
  pseudo_whisper  a whispered copy of a chosen share of the normal clips
                  (scripts/pseudo-whisper.py, LPC + noise resynthesis); the
                  normal clip stays in the set too, so each copy is paired
  whisper         real whispered recordings (the recording kit's whispered
                  pass), added with --real-whisper; never augmented again

Which clips get a whispered copy is decided by a hash of the clip id and
--seed, so a rerun gives the same set. The eval split is held out by sentence
(a hash of the normalised reference), so a sentence and its whispered copy
never land on both sides; with a speaker column, --holdout-speakers moves
whole speakers to eval instead (the recording kit plan holds out two).

Leakage guard: any clip whose normalised reference equals a reference in one
of the bench manifests (bench/audio/*/*.tsv by default, --exclude to add
more) is dropped and counted, so the fine-tune never trains on what the bench
scores. Bench manifests themselves are refused as sources unless
--allow-bench is given (for a smoke test only).

Sources are manifests in the bench format, `id<TAB>lang<TAB>reference` with
an optional fourth `speaker` column, clips (16 kHz mono 16-bit WAV) beside
the manifest. scripts/fetch-fleurs.py --split train writes one for FLEURS
(CC-BY-4.0); any other source must allow training and redistribution of the
derived model (check and record its licence in --licence).

Output (write it outside the repository; audio is never committed):
  <out>/audio/*.wav          normal clips (hard links when possible) and
                             pw-*.wav whispered copies
  <out>/train.jsonl, eval.jsonl
                             one JSON object per clip: audio (relative path),
                             text, language, condition, source, parent (the
                             normal clip a whispered copy came from), speaker
  <out>/stats.json           counts and hours per split and condition,
                             dropped clips, settings, licences

Usage:
  scripts/make-finetune-data.py <out-dir> --source <manifest.tsv>...
      [--real-whisper <manifest.tsv>...] [--whisper-share 0.25]
      [--eval-share 0.05] [--holdout-speakers a,b] [--exclude <tsv>...]
      [--licence "FLEURS: CC-BY-4.0"] [--seed 1] [--jobs 4] [--allow-bench]
"""

import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import sys
import unicodedata
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BENCH = ROOT / "bench" / "audio"


def load_pseudo_whisper():
    spec = importlib.util.spec_from_file_location("pseudo_whisper", Path(__file__).with_name("pseudo-whisper.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def norm(text: str) -> str:
    """Lower case, punctuation off, spaces folded: enough to match a reference."""
    text = unicodedata.normalize("NFC", text).casefold()
    text = re.sub(r"[^\w\s]", " ", text)
    return " ".join(text.split())


def unit(key: str, seed: int) -> float:
    """Deterministic number in [0, 1) for a key."""
    digest = hashlib.sha256(f"{seed}:{key}".encode()).digest()
    return int.from_bytes(digest[:8], "little") / 2**64


def read_manifest(path: Path) -> list[dict]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        cols = line.split("\t")
        if len(cols) < 3:
            raise SystemExit(f"{path}: need id<TAB>lang<TAB>reference, got {line!r}")
        cid = cols[0]
        wav = path.parent / (cid if cid.endswith(".wav") else f"{cid}.wav")
        if not wav.exists():
            raise SystemExit(f"{path}: clip {wav.name} missing")
        rows.append({
            "id": Path(wav.name).stem,
            "wav": wav,
            "language": cols[1],
            "text": cols[2].strip(),
            "speaker": cols[3].strip() if len(cols) > 3 and cols[3].strip() else None,
            "source": path.parent.name,
        })
    return rows


def place(src: Path, dst: Path) -> None:
    if dst.exists():
        return
    try:
        os.link(src, dst)
    except OSError:
        shutil.copyfile(src, dst)


def whisper_job(args: tuple) -> float:
    src, dst, seed, cid = args
    pw = load_pseudo_whisper()
    import numpy as np

    digest = hashlib.sha256(f"{seed}:{cid}".encode()).digest()
    rng = np.random.default_rng(int.from_bytes(digest[:8], "little"))
    x = pw.read_wav(Path(src))
    pw.write_wav(Path(dst), pw.whisperise(x, 150.0, -44.0, rng, 0.9, 6.0, -62.0))
    return len(x) / pw.SR


def seconds(path: Path) -> float:
    import wave

    with wave.open(str(path), "rb") as w:
        return w.getnframes() / w.getframerate()


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("out", type=Path)
    p.add_argument("--source", type=Path, nargs="+", required=True)
    p.add_argument("--real-whisper", type=Path, nargs="*", default=[])
    p.add_argument("--whisper-share", type=float, default=0.25)
    p.add_argument("--eval-share", type=float, default=0.05)
    p.add_argument("--holdout-speakers", default="")
    p.add_argument("--exclude", type=Path, nargs="*", default=[])
    p.add_argument("--licence", action="append", default=[])
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--jobs", type=int, default=4)
    p.add_argument("--allow-bench", action="store_true")
    a = p.parse_args()

    if not 0 <= a.whisper_share <= 1 or not 0 <= a.eval_share < 1:
        p.error("shares must be between 0 and 1")
    out = a.out.resolve()
    if out == ROOT or ROOT in out.parents:
        p.error("write the set outside the repository (audio is never committed)")

    bench_tsvs = sorted(BENCH.glob("*/*.tsv"))
    for src in [*a.source, *a.real_whisper]:
        if src.resolve() in {t.resolve() for t in bench_tsvs} and not a.allow_bench:
            p.error(f"{src} is a bench manifest; training on it would leak into the scores (--allow-bench for a smoke test)")
    banned = set()
    for tsv in [*bench_tsvs, *a.exclude]:
        for line in tsv.read_text(encoding="utf-8").splitlines():
            cols = line.split("\t")
            if len(cols) >= 3:
                banned.add(norm(cols[2]))
    banned.discard("")
    holdout = {s.strip() for s in a.holdout_speakers.split(",") if s.strip()}

    rows, dropped = [], []
    for kind, paths in (("normal", a.source), ("whisper", a.real_whisper)):
        for path in paths:
            for row in read_manifest(path):
                row["condition"] = kind
                if norm(row["text"]) in banned:
                    dropped.append(f"{row['source']}/{row['id']}")
                    continue
                rows.append(row)

    (out / "audio").mkdir(parents=True, exist_ok=True)
    entries, jobs = [], []
    for row in rows:
        split = "eval" if (row["speaker"] in holdout if holdout and row["speaker"]
                           else unit("eval:" + norm(row["text"]), a.seed) < a.eval_share) else "train"
        name = f"{row['source']}-{row['id']}.wav"
        place(row["wav"], out / "audio" / name)
        base = {"text": row["text"], "language": row["language"], "source": row["source"], "speaker": row["speaker"]}
        entries.append((split, {"audio": f"audio/{name}", **base, "condition": row["condition"], "parent": None},
                        seconds(row["wav"])))
        if row["condition"] == "normal" and unit("pw:" + name, a.seed) < a.whisper_share:
            pw_name = f"pw-{name}"
            jobs.append((str(row["wav"]), str(out / "audio" / pw_name), a.seed, name))
            entries.append((split, {"audio": f"audio/{pw_name}", **base, "condition": "pseudo_whisper",
                                    "parent": f"audio/{name}"}, seconds(row["wav"])))

    todo = [j for j in jobs if not Path(j[1]).exists()]
    with ProcessPoolExecutor(max_workers=max(1, a.jobs)) as pool:
        for i, _ in enumerate(pool.map(whisper_job, todo, chunksize=4), 1):
            if i % 50 == 0 or i == len(todo):
                print(f"whispered {i}/{len(todo)}", file=sys.stderr)

    stats = {"seed": a.seed, "whisper_share": a.whisper_share, "eval_share": a.eval_share,
             "holdout_speakers": sorted(holdout), "licences": a.licence,
             "sources": [str(s) for s in a.source], "real_whisper": [str(s) for s in a.real_whisper],
             "dropped_bench_overlap": len(dropped), "dropped_examples": dropped[:10], "splits": {}}
    for split in ("train", "eval"):
        chosen = [(e, s) for sp, e, s in entries if sp == split]
        with open(out / f"{split}.jsonl", "w", encoding="utf-8") as f:
            for e, _ in chosen:
                f.write(json.dumps(e, ensure_ascii=False) + "\n")
        by = {}
        for e, s in chosen:
            c = by.setdefault(e["condition"], {"clips": 0, "hours": 0.0})
            c["clips"] += 1
            c["hours"] += s / 3600
        for c in by.values():
            c["hours"] = round(c["hours"], 3)
        stats["splits"][split] = by
    (out / "stats.json").write_text(json.dumps(stats, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(stats["splits"], indent=None), f"dropped {len(dropped)} bench-overlap clips", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
