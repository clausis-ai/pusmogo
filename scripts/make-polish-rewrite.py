#!/usr/bin/env python3
"""Build light-rewrite pairs for the Hungarian polish model: a sentence as people
say it (fillers, repeats, false starts, self-corrections, no punctuation) and
the same sentence written cleanly.

The clean side is never generated: it is a checked sentence from
scripts/make-polish-text.py. Only the spoken side is made, by rules, so a pair
cannot teach the model to add or change facts. Sentences that the bench scores
are left out.

Usage:
  scripts/make-polish-rewrite.py <out-dir> --sentences business.tsv general.tsv [--extra synth-dir ...]
"""

import argparse
import csv
import importlib.util
import json
import random
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("pairs", ROOT / "scripts/make-polish-pairs.py")
mp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mp)

OPENERS = ["Szóval", "Hát", "Na szóval", "Tehát", "Hát szóval", "Izé"]
FILLERS = ["izé", "hogy is mondjam", "szóval", "hát", "ugye", "mondjuk", "tehát"]
CUES = ["nem", "bocsánat", "vagyis", "akarom mondani", "pontosabban", "illetve nem"]
SWAPS = [["hétfőn", "kedden", "szerdán", "csütörtökön", "pénteken"],
         ["januárban", "februárban", "márciusban", "áprilisban", "májusban", "júniusban", "szeptemberben", "októberben"],
         ["két", "három", "négy", "öt", "hat", "tíz"], ["első", "második", "harmadik"],
         ["tavaly", "idén", "jövőre"], ["reggel", "délután", "este"], ["tegnap", "holnap"]]


def lower_first(s: str) -> str:
    return s[0].lower() + s[1:]


def opener(s, rng):
    return f"{rng.choice(OPENERS)}, {lower_first(s)}"


def filler(s, rng):
    w = s.split()
    i = rng.randrange(1, len(w) - 1)
    return " ".join(w[:i]) + f", {rng.choice(FILLERS)}, " + " ".join(w[i:])


def repeat(s, rng):
    w = s.split()
    n = rng.choice([1, 2, 2, 3])
    i = rng.randrange(0, max(1, len(w) - n))
    part = [re.sub(r"[.,;:?!]+$", "", x) for x in w[i:i + n]]
    return " ".join(w[:i] + part + [lower_first(w[i])] + w[i + 1:])


def false_start(s, rng):
    w = s.split()
    start = [re.sub(r"[.,;:?!]+$", "", x) for x in w[:rng.choice([2, 3, 4])]]
    return " ".join(start) + rng.choice([", szóval ", ", izé, ", "... ", ", na, "]) + lower_first(s)


def self_correct(s, rng):
    for group in rng.sample(SWAPS, len(SWAPS)):
        for right in group:
            m = re.search(rf"(?<!\w){right}(?!\w)", s)
            if m:
                wrong = rng.choice([x for x in group if x != right])
                return f"{s[:m.start()]}{wrong}, {rng.choice(CUES)}, {right}{s[m.end():]}"
    return None


def flat(s, rng):
    return " ".join(re.sub(r"[.,;:?!]", "", s.lower()).split())


def spoken(s, rng):
    sc = self_correct(s, rng)
    out = sc if sc and rng.random() < 0.7 else rng.choice([opener, filler, repeat, false_start])(s, rng)
    if rng.random() < 0.3:
        out = rng.choice([filler, repeat])(out, rng)
    if rng.random() < 0.3:
        out = flat(out, rng)
    return out


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("out", type=Path)
    p.add_argument("--sentences", type=Path, nargs="+", required=True)
    p.add_argument("--extra", type=Path, nargs="*", default=[])
    p.add_argument("--keep-share", type=float, default=0.3)
    p.add_argument("--valid-share", type=float, default=0.03)
    p.add_argument("--seed", type=int, default=1)
    a = p.parse_args()
    out = a.out.resolve()
    if out == ROOT or ROOT in out.parents:
        p.error("write the pairs outside the repository (they hold dataset text)")
    rng = random.Random(a.seed)
    banned, _ = mp.bench_refs_and_ids([])
    sentences = set()
    for path in a.sentences:
        with open(path, encoding="utf-8", newline="") as f:
            sentences |= {r["sentence"] for r in csv.DictReader(f, delimiter="\t", quoting=csv.QUOTE_NONE)}
    sentences = sorted(s for s in sentences if mp.norm(s) not in banned)
    rng.shuffle(sentences)
    splits = {"train": [], "valid": []}
    kinds = {"rewrite": 0, "keep": 0}
    for s in sentences:
        keep = rng.random() < a.keep_share
        kinds["keep" if keep else "rewrite"] += 1
        side = "valid" if mp.unit("split:" + mp.norm(s), a.seed) < a.valid_share else "train"
        splits[side].append(json.dumps(mp.pair(s if keep else spoken(s, rng), s), ensure_ascii=False))
    out.mkdir(parents=True, exist_ok=True)
    for name, rows in splits.items():
        for extra in a.extra:
            rows += (extra / f"{name}.jsonl").read_text(encoding="utf-8").splitlines()
        rng.shuffle(rows)
        (out / f"{name}.jsonl").write_text("\n".join(rows) + "\n", encoding="utf-8")
    print(kinds, {n: len(r) for n, r in splits.items()}, file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
