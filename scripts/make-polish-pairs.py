#!/usr/bin/env python3
"""Build Hungarian correction pairs (Whisper output -> reference) for a polish fine-tune.

A small model that fixes Whisper's Hungarian mistakes has to learn them from
Whisper's own output, not from text with made-up errors. This script turns
pusmogo-bench runs over training audio into chat-format pairs that MLX-LM's
LoRA trainer reads directly (docs/bench/polish-08.md has the recipe):

  user      what the app would type: Whisper's text after the core's clean()
  assistant the human reference for the same clip

Run pusmogo-bench with --json over a training manifest first (FLEURS train via
scripts/fetch-fleurs.py --split train, or Common Voice HU, or a set built with
scripts/make-finetune-data.py); then pass the JSON files here.

Filters, each counted in stats.json:
  empty      no output (nothing to correct)
  too_wrong  word error rate above --max-wer (0.5): a decode that far off is
             a hallucination or the wrong audio, and teaching the model to
             rewrite it would teach it to invent text
  keep_cap   pairs where output already equals the reference are kept only up
             to --keep-share of the final set (0.3), so the model learns to
             leave right text alone without learning only that

Leakage guard: a pair whose normalised reference matches a reference in any
bench manifest (bench/audio/*/*.tsv), the polish sets (bench/polish/*.tsv) or
--exclude is dropped, and a JSON run over a bench manifest (same clip ids) is
refused unless --allow-bench is given (smoke test only).

Split by sentence (hash of the normalised reference and --seed): every take
of a sentence lands on the same side. Output (outside the repository):
  <out>/train.jsonl, valid.jsonl, test.jsonl   {"messages": [system, user, assistant]}
  <out>/stats.json                             counts, filters, WER before, licences;
                                               fix_words (a word differs) vs
                                               fix_format_only (punctuation/case)

Usage:
  scripts/make-polish-pairs.py <out-dir> --runs <bench.json>...
      [--max-wer 0.5] [--keep-share 0.3] [--valid-share 0.05]
      [--test-share 0.05] [--exclude <tsv>...] [--licence "FLEURS: CC-BY-4.0"]
      [--seed 1] [--allow-bench]
"""

import argparse
import hashlib
import json
import re
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BENCH_TSVS = sorted((ROOT / "bench" / "audio").glob("*/*.tsv")) + sorted((ROOT / "bench" / "polish").glob("*.tsv"))

# Short on purpose: every training token of the prompt is paid on every step
# and on every dictation later. The behaviour comes from the pairs.
# The polish model's whole job, stated once: Hungarian
# only, dictation spoken or whispered into a microphone, make it proper Hungarian,
# never change the meaning. Every token here is paid on each dictation, so it
# stays short; the behaviour itself comes from the pairs.
SYSTEM = (
    "Egyetlen feladatod: magyar nyelvű diktálás gépi leiratát helyes magyar szöveggé javítani. "
    "A szöveget valaki mikrofonba mondta vagy suttogta, és a beszédfelismerő néha félrehall. "
    "Javítsd a félrehallott szavakat, a ragokat, a helyesírást és az írásjeleket. "
    "Hagyd el a töltelékszavakat, az ismétléseket és az elkezdett, majd kijavított részeket. "
    "A jelentést ne változtasd, és ne tegyél hozzá semmit. "
    "Az üzleti angol szavakat (például deadline, meeting, feedback) hagyd angolul, "
    "és a helyesírási szabály szerint ragozd őket: deadline-ra, meetingen, feedbacket. "
    "Csak magyarul dolgozol. Csak a javított szöveget írd ki."
)


def norm(text: str) -> str:
    text = unicodedata.normalize("NFC", text).casefold()
    text = re.sub(r"[^\w\s]", " ", text)
    return " ".join(text.split())


def unit(key: str, seed: int) -> float:
    digest = hashlib.sha256(f"{seed}:{key}".encode()).digest()
    return int.from_bytes(digest[:8], "little") / 2**64


def bench_refs_and_ids(extra: list[Path]) -> tuple[set[str], set[str]]:
    refs, ids = set(), set()
    for tsv in [*BENCH_TSVS, *extra]:
        for line in tsv.read_text(encoding="utf-8").splitlines():
            cols = line.split("\t")
            if len(cols) >= 3 and not line.startswith("#"):
                refs.add(norm(cols[2]))
                if tsv in BENCH_TSVS:
                    ids.add(Path(cols[0]).stem)
    refs.discard("")
    return refs, ids


def pair(user: str, assistant: str) -> dict:
    return {"messages": [{"role": "system", "content": SYSTEM},
                         {"role": "user", "content": user},
                         {"role": "assistant", "content": assistant}]}


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("out", type=Path)
    p.add_argument("--runs", type=Path, nargs="+", required=True)
    p.add_argument("--max-wer", type=float, default=0.5)
    p.add_argument("--keep-share", type=float, default=0.3)
    p.add_argument("--valid-share", type=float, default=0.05)
    p.add_argument("--test-share", type=float, default=0.05)
    p.add_argument("--exclude", type=Path, nargs="*", default=[])
    p.add_argument("--licence", action="append", default=[])
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--allow-bench", action="store_true")
    a = p.parse_args()

    if not 0 <= a.keep_share < 1 or a.valid_share + a.test_share >= 1:
        p.error("shares must be between 0 and 1, and valid + test below 1")
    out = a.out.resolve()
    if out == ROOT or ROOT in out.parents:
        p.error("write the pairs outside the repository (they hold dataset text)")

    banned, bench_ids = bench_refs_and_ids(a.exclude)
    counts = {"clips": 0, "empty": 0, "too_wrong": 0, "bench_overlap": 0, "keep_capped": 0}
    fixes, keeps, models = [], [], set()
    errors = words = 0
    for run in a.runs:
        data = json.loads(run.read_text(encoding="utf-8"))
        models.add(Path(data.get("model", "?")).name)
        clips = data["clips"]
        if not a.allow_bench and sum(Path(c["id"]).stem in bench_ids for c in clips) > 0:
            p.error(f"{run} was run over bench clips; training on it would leak into the scores (--allow-bench for a smoke test)")
        if any("\t" in c["reference"] for c in clips):
            p.error(f"{run}: a reference holds a tab (a speaker column read as text): its WER and its pairs are wrong, rerun the bench")
        for c in clips:
            counts["clips"] += 1
            hyp, ref = c["output"].strip(), c["reference"].strip()
            if norm(ref) in banned:
                counts["bench_overlap"] += 1
                continue
            if not norm(hyp):
                counts["empty"] += 1
                continue
            if c["wer"]["rate"] > a.max_wer:
                counts["too_wrong"] += 1
                continue
            errors += c["wer"]["errors"]
            words += c["wer"]["len"]
            (keeps if hyp == ref else fixes).append((hyp, ref))

    # Cap the right-as-is pairs so they are at most keep_share of the set;
    # which ones stay is decided by hash, so a rerun keeps the same ones.
    cap = int(a.keep_share / (1 - a.keep_share) * len(fixes)) if a.keep_share else 0
    keeps.sort(key=lambda hr: unit("keep:" + hr[0], a.seed))
    counts["keep_capped"] = max(0, len(keeps) - cap)
    keeps = keeps[:cap]

    splits = {"train": [], "valid": [], "test": []}
    for hyp, ref in fixes + keeps:
        u = unit("split:" + norm(ref), a.seed)
        split = "test" if u < a.test_share else "valid" if u < a.test_share + a.valid_share else "train"
        splits[split].append((hyp, ref))
    out.mkdir(parents=True, exist_ok=True)
    stats = {"seed": a.seed, "max_wer": a.max_wer, "keep_share": a.keep_share, "system": SYSTEM,
             "runs": [str(r) for r in a.runs], "models": sorted(models), "licences": a.licence,
             "filtered": counts, "kept_wer": round(errors / words, 4) if words else None, "splits": {}}
    for name, rows in splits.items():
        rows.sort(key=lambda hr: unit("order:" + hr[0] + hr[1], a.seed))
        with open(out / f"{name}.jsonl", "w", encoding="utf-8") as f:
            for hyp, ref in rows:
                f.write(json.dumps(pair(hyp, ref), ensure_ascii=False) + "\n")
        stats["splits"][name] = {"pairs": len(rows), "keep": sum(h == r for h, r in rows),
                                 "fix_words": sum(norm(h) != norm(r) for h, r in rows),
                                 "fix_format_only": sum(h != r and norm(h) == norm(r) for h, r in rows)}
    (out / "stats.json").write_text(json.dumps(stats, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(stats["splits"]), json.dumps(counts), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
