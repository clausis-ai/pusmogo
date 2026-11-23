#!/usr/bin/env python3
"""Build synthetic correction pairs for the polish model: Hungarian grammar and
English business terms inside Hungarian sentences.

Two kinds, in the same chat format as `scripts/make-polish-pairs.py` (same
system prompt), so they can be mixed into its train/valid files:

1. Grammar: a correct Hungarian sentence (Common Voice validated_sentences.tsv,
   CC0) with one typical slip put in, the sentence as it was as the answer.
   Slips follow rules of "A magyar helyesírás szabályai" (AkH. 12, 2015): the
   comma before a subordinate clause (hogy, mert, ami, aki, amely, ahol, amikor,
   mivel, ha) and before de/pedig; j for ly; a short vowel for a long one
   (ö/ő, ü/ű, i/í, u/ú, o/ó); a single consonant for a double one.
2. English terms: business English words kept in English and suffixed the
   Hungarian way (AkH. 217: hyphen after a silent letter, deadline-ra), from
   `scripts/data/polish-en-terms.tsv`. Inputs are what a speech engine types
   (dedlájnra, de ad linera, deadlinera); right-as-is pairs teach the model to
   leave a correct English term alone. Terms marked "!" never enter training:
   their sentences go to heldout-en.tsv for the bench.

A share of right-as-is pairs (--keep-share) teaches the model to change nothing
when nothing is wrong.

Usage:
  scripts/make-polish-synth.py <out-dir> --sentences <cv>/hu/validated_sentences.tsv [more.tsv ...]
      [--en-context en-context.tsv] [--grammar 12000]

More sentence files and --en-context come from `scripts/make-polish-text.py`
(clean business Hungarian from web text): their sentences get the same slips,
and each en-context sentence gets the engine's misheard forms of its term.
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

TERMS = ROOT / "scripts/data/polish-en-terms.tsv"
CASES = ["acc", "sup", "sub", "ine"]
TEMPLATES = {
    "acc": ["Kérlek, nézd át {art} {X} holnapig.", "Elküldtem {art} {X} a csapatnak.",
            "Frissítettem {art} {X}, mert változott a terv.", "Mikorra kéred {art} {X}?",
            "Átbeszéltük {art} {X} Annával, és szerinte jó."],
    "sup": ["Már dolgozom {art} {X}, holnapra kész lesz.", "Ezt {art} {X} is megnéztük.",
            "Szerintem {art} {X} még változtatni kell."],
    "sub": ["Kérlek, figyelj oda {art} {X}, mert fontos.", "Erre {art} {X} még visszatérünk.",
            "Holnap reggel ránézek {art} {X}."],
    "ine": ["{Art} {X} van még egy hiba, amit javítani kell.", "Ezt {art} {X} is jeleztem.",
            "Minden benne van {art} {X}, amit kértél."],
}
SUBORDINATORS = r"(hogy|mert|ami\w*|aki\w*|amely\w*|ahol|amikor|mivel|ha|de|pedig)"
LONG_TO_SHORT = {"ő": "ö", "ű": "ü", "í": "i", "ú": "u", "ó": "o"}


def read_terms() -> list[dict]:
    rows = []
    for line in TERMS.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        c = line.split("\t")
        held = c[0].startswith("!")
        rows.append({"term": c[0].lstrip("!"), "held": held, "art": c[1],
                     "forms": dict(zip(CASES, c[2:6])), "misheard": [m for m in c[6].split("|") if m]})
    return rows


def suffix_of(term: str, form: str) -> tuple[str, bool]:
    """The suffix of a correct form and whether a hyphen joins it."""
    rest = form[len(term):]
    return (rest[1:], True) if rest.startswith("-") else (rest, False)


def fill(template: str, art: str, x: str) -> str:
    return template.replace("{Art}", art.capitalize()).replace("{art}", art).replace("{X}", x)


def term_pairs(terms: list[dict], rng: random.Random) -> tuple[list[tuple[str, str]], list[str]]:
    pairs, held = [], []
    for t in terms:
        for case in CASES:
            form = t["forms"][case]
            if not form:
                continue
            suffix, hyphen = suffix_of(t["term"], form)
            for template in TEMPLATES[case]:
                right = fill(template, t["art"], form)
                if t["held"]:
                    held.append(right)
                    continue
                wrongs = [fill(template, t["art"], m + suffix) for m in t["misheard"]]
                if hyphen:
                    wrongs.append(fill(template, t["art"], t["term"] + suffix))
                for w in wrongs:
                    pairs.append((w, right))
                pairs.append((right, right))
    return pairs, held


def context_pairs(terms: list[dict], path: Path, banned: set[str]) -> list[tuple[str, str]]:
    """Real sentences that use a term right: each misheard form, the form
    without its hyphen, and the sentence itself as a right-as-is pair."""
    by_term = {t["term"]: t for t in terms if not t["held"]}
    pairs = []
    with open(path, encoding="utf-8", newline="") as f:
        for r in csv.DictReader(f, delimiter="\t", quoting=csv.QUOTE_NONE):
            t = by_term.get(r["term"])
            s, form = r["sentence"], r["form"]
            if not t or form not in s or mp.norm(s) in banned:
                continue
            suffix, hyphen = suffix_of(t["term"], form)
            wrongs = [m + suffix for m in t["misheard"]] + ([t["term"] + suffix] if hyphen else [])
            pairs += [(s.replace(form, w, 1), s) for w in wrongs]
            pairs.append((s, s))
    return pairs


def corrupt(sentence: str, rng: random.Random) -> str | None:
    """One slip, or None when this sentence has nothing the chosen slip can touch."""
    kinds = ["comma", "ly", "long", "double"]
    rng.shuffle(kinds)
    words = sentence.split(" ")
    for kind in kinds:
        if kind == "comma":
            hits = list(re.finditer(r", " + SUBORDINATORS + r"\b", sentence))
            if hits:
                m = rng.choice(hits)
                return sentence[:m.start()] + sentence[m.start() + 1:]
        else:
            idx = [i for i, w in enumerate(words)
                   if (kind == "ly" and "ly" in w)
                   or (kind == "long" and any(ch in w for ch in LONG_TO_SHORT))
                   or (kind == "double" and re.search(r"([bcdfghjklmnprstvz])\1", w))]
            if not idx:
                continue
            i = rng.choice(idx)
            w = words[i]
            if kind == "ly":
                w = w.replace("ly", "j", 1)
            elif kind == "long":
                ch = rng.choice([c for c in w if c in LONG_TO_SHORT])
                w = w.replace(ch, LONG_TO_SHORT[ch], 1)
            else:
                w = re.sub(r"([bcdfghjklmnprstvz])\1", r"\1", w, count=1)
            return " ".join(words[:i] + [w] + words[i + 1:])
    return None


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("out", type=Path)
    p.add_argument("--sentences", type=Path, nargs="+", required=True)
    p.add_argument("--en-context", type=Path)
    p.add_argument("--grammar", type=int, default=12000)
    p.add_argument("--keep-share", type=float, default=0.3)
    p.add_argument("--valid-share", type=float, default=0.05)
    p.add_argument("--seed", type=int, default=1)
    a = p.parse_args()
    out = a.out.resolve()
    if out == ROOT or ROOT in out.parents:
        p.error("write the pairs outside the repository (they hold dataset text)")
    rng = random.Random(a.seed)
    banned, _ = mp.bench_refs_and_ids([])

    sentences = []
    for path in a.sentences:
        with open(path, encoding="utf-8", newline="") as f:
            sentences += [" ".join(r["sentence"].split()) for r in csv.DictReader(f, delimiter="\t", quoting=csv.QUOTE_NONE)]
    sentences = sorted({s for s in sentences if 4 <= len(s.split()) <= 30 and mp.norm(s) not in banned})
    rng.shuffle(sentences)
    grammar, keeps = [], []
    for s in sentences:
        if len(grammar) >= a.grammar:
            break
        w = corrupt(s, rng)
        if w and w != s:
            grammar.append((w, s))
    for s in sentences[len(grammar):]:
        if len(keeps) >= int(a.keep_share / (1 - a.keep_share) * len(grammar)):
            break
        keeps.append((s, s))

    terms = read_terms()
    en, held = term_pairs(terms, rng)
    if a.en_context:
        en += context_pairs(terms, a.en_context, banned)
    rows = grammar + keeps + en
    splits = {"train": [], "valid": []}
    for hyp, ref in rows:
        splits["valid" if mp.unit("split:" + mp.norm(ref), a.seed) < a.valid_share else "train"].append((hyp, ref))
    out.mkdir(parents=True, exist_ok=True)
    for name, part in splits.items():
        part.sort(key=lambda hr: mp.unit("order:" + hr[0] + hr[1], a.seed))
        with open(out / f"{name}.jsonl", "w", encoding="utf-8") as f:
            for hyp, ref in part:
                f.write(json.dumps(mp.pair(hyp, ref), ensure_ascii=False) + "\n")
    (out / "heldout-en.tsv").write_text("\n".join(held) + "\n", encoding="utf-8")
    stats = {"grammar": len(grammar), "keeps": len(keeps), "en_terms": len(en),
             "en_terms_trained": sum(not t["held"] for t in terms), "heldout_en": len(held),
             "train": len(splits["train"]), "valid": len(splits["valid"])}
    (out / "stats.json").write_text(json.dumps(stats, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(stats), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
