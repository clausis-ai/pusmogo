#!/usr/bin/env python3
"""Ask a small local language model to fix misheard Hungarian words, and score it.

Set: bench/polish/misheard-hu.tsv. Each input is a real large-v3 output from the
bench (FLEURS HU, short-01, noise sets); the reference is the same text with only
the misheard words put right. "keep" items are already right and must not change.

The model sits behind a llama.cpp server (OpenAI-style chat endpoint), as for
scripts/polish-llm.py. It gets the rule-cleaned text and a prompt that allows
only replacing a word with a similar-sounding one.

The sound-alike check (`soundalike_guard`) sits between the model and insertion:
  - the number of question marks stays the same;
  - no digit may be added, dropped or changed;
  - words are aligned with difflib; every changed run holds at most 3 words on
    each side, and the run before and after (letters only, accents folded,
    spaces removed) must have a character similarity of at least MIN_SIM, so
    "Holnagy" -> "Holnap" or "Leveletírók" -> "Levelet írok" pass, and an added
    or dropped word on its own does not;
  - at most a third of the typed words (and at least two) may change.
When the check fails, the rule-cleaned text is typed.

Systems: raw (engine output), rules (Pusmogo today), oracle (references, a test
of the check), and with --port the model, before and after the check.
Metrics per kind: WER against the reference (lower-cased, no punctuation),
items fully right, items fixed (wrong before, right after), items broken (a word
that was right before is wrong after; for keep items: any word change).

Usage:
  scripts/polish-misheard.py [--port 8090 --name MODEL [--eager]] [--out DIR]
"""

from __future__ import annotations

import argparse
import difflib
import importlib.util
import json
import statistics
import time
import unicodedata
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SET = ROOT / "bench/polish/misheard-hu.tsv"
spec = importlib.util.spec_from_file_location("score_polish", ROOT / "scripts/score-polish.py")
sp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sp)

MIN_SIM = 0.5

SYSTEM = (
    "Magyar beszédfelismerő kimenetét javítod. A gép néha félrehall egy szót: "
    "hasonló hangzású, de rossz vagy nem létező szót ír helyette.\n"
    "Szabályok:\n"
    "1. Csak az ilyen félrehallott szavakat javítsd, a hangzásban hozzájuk legközelebb álló helyes magyar szóra.\n"
    "2. Más szót ne adj hozzá, ne hagyj el és ne fogalmazz át. Az írásjeleket és a számokat hagyd úgy, ahogy vannak.\n"
    "3. A neveket és az idegen szavakat ne cseréld le.\n"
    "4. Ha minden szó helyes, add vissza változatlanul.\n"
    "Csak a javított szöveget írd ki, magyarázat nélkül."
)

# Worked examples, deliberately different from every item of the set.
SHOTS = [
    ("Tegnap a bót be mentem kenyérért.", "Tegnap a boltba mentem kenyérért."),
    ("A gyerekek a jácótéren játszanak.", "A gyerekek a játszótéren játszanak."),
    ("A vonat tíz perc késéssel érkezett Debrecenbe.", "A vonat tíz perc késéssel érkezett Debrecenbe."),
    ("Kérlek, küld el a számlát pénteking.", "Kérlek, küldd el a számlát péntekig."),
    ("Ma este Kovácsné főzi a vacsorát.", "Ma este Kovácsné főzi a vacsorát."),
]


def read_set() -> list[dict]:
    rows = []
    for line in SET.read_text(encoding="utf-8").splitlines():
        if line.strip() and not line.startswith("#"):
            rid, kind, inp, ref = line.split("\t")
            rows.append({"id": rid, "kind": kind, "input": inp, "ref": ref})
    return rows


def fold(words: list[str]) -> str:
    s = unicodedata.normalize("NFD", "".join(words))
    return "".join(ch for ch in s if ch.isalnum() and not unicodedata.combining(ch))


def soundalike_guard(candidate: str, rules: str) -> bool:
    """True when the candidate may be typed instead of the rule-cleaned text."""
    if not candidate.strip():
        return False
    if candidate.count("?") != rules.count("?"):
        return False
    if [ch for ch in candidate if ch.isdigit()] != [ch for ch in rules if ch.isdigit()]:
        return False
    c, r = sp.words(candidate), sp.words(rules)
    changed = 0
    for op, i1, i2, j1, j2 in difflib.SequenceMatcher(a=r, b=c, autojunk=False).get_opcodes():
        if op == "equal":
            continue
        if op != "replace" or i2 - i1 > 3 or j2 - j1 > 3:
            return False
        if difflib.SequenceMatcher(a=fold(r[i1:i2]), b=fold(c[j1:j2])).ratio() < MIN_SIM:
            return False
        changed += i2 - i1
    return changed <= max(2, len(r) // 3)


# A variant that tells the model an error is likely and shows only changing
# examples: does the model fail to find errors, or does it not dare to fix them?
EAGER = SYSTEM.replace("Ha minden szó helyes, add vissza változatlanul.",
                       "A szövegben valószínűleg van legalább egy félrehallott szó: keresd meg és javítsd.")


def ask(port: int, text: str, eager: bool = False) -> str:
    msgs = [{"role": "system", "content": EAGER if eager else SYSTEM}]
    for q, a in SHOTS:
        if eager and q == a:
            continue
        msgs += [{"role": "user", "content": q}, {"role": "assistant", "content": a}]
    msgs.append({"role": "user", "content": text})
    body = {"messages": msgs, "temperature": 0, "max_tokens": 8 * len(text.split()) + 16,
            "chat_template_kwargs": {"enable_thinking": False}}
    req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",
                                 data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        out = json.load(r)["choices"][0]["message"]["content"]
    if "</think>" in out:
        out = out.split("</think>", 1)[1]
    lines = [ln.strip() for ln in out.strip().splitlines() if ln.strip()]
    return lines[-1] if lines else ""


def wrong_positions(hyp: str, ref: str) -> set[int]:
    """Reference word positions that the hypothesis does not get right."""
    r, h = sp.words(ref), sp.words(hyp)
    right = set()
    for op, i1, i2, _, _ in difflib.SequenceMatcher(a=r, b=h, autojunk=False).get_opcodes():
        if op == "equal":
            right.update(range(i1, i2))
    return set(range(len(r))) - right


def score(outs: list[str], base: list[str], rows: list[dict]) -> dict[str, dict]:
    res = {}
    for kind in ["short", "sentence", "keep", "all"]:
        idx = [i for i, r in enumerate(rows) if kind in ("all", r["kind"])]
        errs = sum(sp.edit_distance(sp.words(outs[i]), sp.words(rows[i]["ref"])) for i in idx)
        n = sum(len(sp.words(rows[i]["ref"])) for i in idx)
        right = sum(not wrong_positions(outs[i], rows[i]["ref"]) and len(sp.words(outs[i])) == len(sp.words(rows[i]["ref"])) for i in idx)
        fixed = broken = 0
        for i in idx:
            before, after = wrong_positions(base[i], rows[i]["ref"]), wrong_positions(outs[i], rows[i]["ref"])
            if rows[i]["kind"] == "keep":
                broken += sp.words(outs[i]) != sp.words(base[i])
            else:
                fixed += bool(before - after)
                broken += bool(after - before)
        res[kind] = {"wer": 100 * errs / n, "right": right, "n": len(idx), "fixed": fixed, "broken": broken}
    return res


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--port", type=int)
    ap.add_argument("--name", default="model")
    ap.add_argument("--out", default="/tmp/polish-misheard")
    ap.add_argument("--eager", action="store_true", help="prompt variant: an error is likely")
    args = ap.parse_args()
    rows = read_set()
    raw = [r["input"] for r in rows]
    rules = sp.clean_all(raw)
    systems = {"raw": raw, "rules": rules}
    oracle = [r["ref"] for r in rows]
    systems["oracle"] = oracle
    passed = sum(soundalike_guard(o, b) for o, b in zip(oracle, rules))
    print(f"oracle passes the sound-alike check: {passed}/{len(rows)}")
    for r, o, b in zip(rows, oracle, rules):
        if not soundalike_guard(o, b):
            print(f"  oracle blocked: {r['id']}: {o}")
    if args.port:
        out_dir = Path(args.out)
        out_dir.mkdir(parents=True, exist_ok=True)
        ask(args.port, rules[0])  # warm up
        cand, secs = [], []
        for t in rules:
            t0 = time.perf_counter()
            cand.append(ask(args.port, t, args.eager).replace("\n", " "))
            secs.append(time.perf_counter() - t0)
        (out_dir / f"{args.name}.txt").write_text("\n".join(cand) + "\n", encoding="utf-8")
        guarded = [c if soundalike_guard(c, b) else b for c, b in zip(cand, rules)]
        systems[args.name] = cand
        systems[args.name + "+check"] = guarded
        q = statistics.quantiles(secs, n=20)
        print(f"added wait: p50 {statistics.median(secs):.3f}s p95 {q[18]:.3f}s max {max(secs):.3f}s")
        for r, c, g, b in zip(rows, cand, guarded, rules):
            if sp.words(c) != sp.words(b):
                tag = "PASS" if g == c else "BLOCK"
                print(f"  {tag} {r['kind']:8} {r['id']}: {b}  =>  {c}")
    print("\n| system | kind | WER | right | fixed | broken |\n|---|---|---:|---:|---:|---:|")
    for name, outs in systems.items():
        for kind, s in score(outs, rules, rows).items():
            print(f"| {name} | {kind} | {s['wer']:.1f} % | {s['right']}/{s['n']} | {s['fixed']} | {s['broken']} |")


if __name__ == "__main__":
    main()
