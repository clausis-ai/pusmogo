#!/usr/bin/env python3
"""Score a text polish pass (punctuation, self-corrections) on Hungarian dictation.

Sets:
  bench/polish/hu.tsv   40 hand-written items: self-corrections, missing clause
                        and list commas, and a safety subset that must not change.
  --bench FILE.json     outputs stored by pusmogo-bench (e.g. FLEURS HU): the
                        output is the input of the polish pass, the reference
                        is the transcript. Several files may be given.

Systems scored on each set:
  raw      the input as the speech engine typed it (polish set only)
  rules    Pusmogo's rule cleanup (pusmogo-mcp clean_text), today's output
  oracle   the references themselves (sanity check: 0 % WER, F1 1.0)
  backtrack  with --backtrack: rules plus the self-correction rule pass
           (core `backtrack.rs`, clean_text "backtrack": true)
  NAME     each --text NAME=FILE: one line per item, in set order

Metrics per subset:
  WER      word errors after lower-casing and dropping punctuation
  exact    outputs equal to the reference, character for character
  P/R/F1   punctuation marks after each word (, . ? ! : ;), words aligned by
           difflib on normalised words; a mark counts only on aligned words
  F1(,)    the same for commas alone
  changed  safety items that differ from the reference (any is a regression)

--guard applies the safety check that would sit between a polish model and
insertion, and scores what would be typed: the candidate when it passes, the
rule-cleaned text when it does not. The check (see `guard()`): the candidate
must keep the number of question marks; it may change other punctuation and
casing freely and never add or replace a word; it may drop a run of at most 5
words only when the run ends in a self-correction cue ("nem", "vagyis",
"bocsánat", "akarom mondani", ...), holds at least one word before the cue and
is followed by a kept word, as in "háromkor, nem, négykor" -> "négykor".

Usage:
  scripts/score-polish.py [--guard] [--backtrack] [--text NAME=FILE ...] [--bench FILE.json ...]
"""

from __future__ import annotations

import argparse
import difflib
import json
import subprocess
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SET = ROOT / "bench/polish/hu.tsv"
MCP = ROOT / "core/target/release/pusmogo-mcp"
MARKS = ",.?!:;"
CUES = [
    "nem", "vagyis", "vagyis inkább", "bocsánat", "bocs", "akarom mondani",
    "javítom", "illetve inkább", "mégis", "helyesebben", "pontosabban",
]


def read_set() -> list[dict]:
    rows = []
    for line in SET.read_text(encoding="utf-8").splitlines():
        if line.strip() and not line.startswith("#"):
            rid, kind, inp, ref = line.split("\t")
            rows.append({"id": rid, "kind": kind, "input": inp, "ref": ref})
    return rows


def read_bench(paths: list[str]) -> list[dict]:
    """Bench clips; clips of the polish set (bench/audio/polish/) keep its kind."""
    kinds = {r["id"]: r["kind"] for r in read_set()}
    rows = []
    for p in paths:
        for c in json.loads(Path(p).read_text(encoding="utf-8"))["clips"]:
            if c.get("lang", "hu") == "hu" and c.get("reference"):
                rows.append({"id": c["id"], "kind": kinds.get(c["id"], c.get("set", "bench")), "input": c["output"], "ref": c["reference"]})
    return rows


def clean_all(texts: list[str], backtrack: bool = False) -> list[str]:
    if not MCP.exists():
        sys.exit(f"missing {MCP}: run (cd core && cargo build --release -p pusmogo-mcp)")
    msgs = [{"jsonrpc": "2.0", "id": 0, "method": "initialize",
             "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "score-polish", "version": "1"}}}]
    for i, t in enumerate(texts):
        msgs.append({"jsonrpc": "2.0", "id": i + 1, "method": "tools/call",
                     "params": {"name": "clean_text", "arguments": {"text": t, "backtrack": backtrack}}})
    out = subprocess.run([str(MCP)], input="\n".join(json.dumps(m) for m in msgs) + "\n",
                         capture_output=True, text=True, check=True).stdout
    res = {}
    for line in out.splitlines():
        r = json.loads(line)
        if r.get("id", 0) > 0:
            res[r["id"]] = r["result"]["content"][0]["text"]
    return [res[i + 1] for i in range(len(texts))]


def tokens(text: str) -> list[tuple[str, str]]:
    """(normalised word, mark after it) pairs."""
    out = []
    for raw in text.split():
        mark = ""
        while raw and raw[-1] in MARKS + "\"'”)":
            if raw[-1] in MARKS and not mark:
                mark = raw[-1]
            raw = raw[:-1]
        word = unicodedata.normalize("NFC", raw.lower()).strip("\"'„“(")
        if word:
            out.append((word, mark))
        elif mark and out and not out[-1][1]:
            out[-1] = (out[-1][0], mark)
    return out


def words(text: str) -> list[str]:
    return [w for w, _ in tokens(text)]


def edit_distance(a: list[str], b: list[str]) -> int:
    prev = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        cur = [i]
        for j, y in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (x != y)))
        prev = cur
    return prev[-1]


def ends_with_cue(span: list[str]) -> bool:
    """A self-correction deletes "<wrong part> <cue>": the span must end in a cue
    and hold at least one word before it (dropping the cue alone is no correction)."""
    return any(span[-len(c.split()):] == c.split() for c in CUES if len(c.split()) < len(span))


def guard(candidate: str, rules: str) -> bool:
    """True when the candidate may be typed instead of the rule-cleaned text."""
    if not candidate.strip():
        return False
    # A question stays a question and a statement a statement: dropping or
    # adding "?" changes what the user asked ("Ház vagy lakás?" -> ".").
    if candidate.count("?") != rules.count("?"):
        return False
    c, r = words(candidate), words(rules)
    ops = difflib.SequenceMatcher(a=r, b=c, autojunk=False).get_opcodes()
    if any(op in ("insert", "replace") for op, *_ in ops):
        return False
    # The correction must follow the dropped run: a run at the very end
    # ("jó ötlet vagy nem?") is content, not a self-correction.
    spans = [(r[i1:i2], i2 < len(r)) for op, i1, i2, _, _ in ops if op == "delete"]
    return all(len(s) <= 5 and followed and ends_with_cue(s) for s, followed in spans)


def punct_counts(hyp: str, ref: str, only: str | None = None) -> tuple[int, int, int]:
    h, r = tokens(hyp), tokens(ref)
    sm = difflib.SequenceMatcher(a=[w for w, _ in r], b=[w for w, _ in h], autojunk=False)
    tp = fp = fn = 0
    aligned_r, aligned_h = set(), set()
    for blk in sm.get_matching_blocks():
        for k in range(blk.size):
            aligned_r.add(blk.a + k)
            aligned_h.add(blk.b + k)
            rm, hm = r[blk.a + k][1], h[blk.b + k][1]
            if only:
                rm, hm = (rm if rm == only else ""), (hm if hm == only else "")
            if rm and rm == hm:
                tp += 1
            else:
                fp += bool(hm)
                fn += bool(rm)
    for i, (_, m) in enumerate(r):
        if i not in aligned_r and m and (not only or m == only):
            fn += 1
    for i, (_, m) in enumerate(h):
        if i not in aligned_h and m and (not only or m == only):
            fp += 1
    return tp, fp, fn


def f1(tp: int, fp: int, fn: int) -> tuple[float, float, float]:
    p = tp / (tp + fp) if tp + fp else 1.0
    r = tp / (tp + fn) if tp + fn else 1.0
    return p, r, (2 * p * r / (p + r) if p + r else 0.0)


def score(outputs: list[str], rows: list[dict]) -> dict:
    res = {}
    for kind in ["all"] + sorted({r["kind"] for r in rows}):
        idx = [i for i, r in enumerate(rows) if kind == "all" or r["kind"] == kind]
        err = sum(edit_distance(words(outputs[i]), words(rows[i]["ref"])) for i in idx)
        n = sum(len(words(rows[i]["ref"])) for i in idx)
        allp = [punct_counts(outputs[i], rows[i]["ref"]) for i in idx]
        com = [punct_counts(outputs[i], rows[i]["ref"], ",") for i in idx]
        p, rc, f = f1(*map(sum, zip(*allp)))
        _, _, fc = f1(*map(sum, zip(*com)))
        exact = sum(outputs[i] == rows[i]["ref"] for i in idx)
        res[kind] = {"n": len(idx), "wer": err / n if n else 0.0, "exact": exact,
                     "p": p, "r": rc, "f1": f, "f1c": fc}
    return res


def table(results: dict[str, dict], rows: list[dict], title: str) -> str:
    kinds = ["all"] + sorted({r["kind"] for r in rows})
    lines = [f"### {title}", "", "| system | subset | n | WER | exact | P | R | F1 | F1(,) |",
             "|---|---|---:|---:|---:|---:|---:|---:|---:|"]
    for name, res in results.items():
        for k in kinds:
            s = res[k]
            lines.append(f"| {name} | {k} | {s['n']} | {s['wer']*100:.1f} % | {s['exact']} | "
                         f"{s['p']:.2f} | {s['r']:.2f} | {s['f1']:.2f} | {s['f1c']:.2f} |")
    return "\n".join(lines)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--text", action="append", default=[], metavar="NAME=FILE")
    ap.add_argument("--bench", action="append", default=[], metavar="FILE.json")
    ap.add_argument("--guard", action="store_true")
    ap.add_argument("--backtrack", action="store_true",
                    help="add the rule pass for self-corrections (clean_text backtrack) as a system")
    ap.add_argument("--show", action="store_true", help="print every item that differs from the reference")
    args = ap.parse_args()

    rows = read_bench(args.bench) if args.bench else read_set()
    rules = clean_all([r["input"] for r in rows])
    systems: dict[str, list[str]] = {}
    if not args.bench:
        systems["raw"] = [r["input"] for r in rows]
    systems["rules"] = rules
    if args.backtrack:
        bt = clean_all([r["input"] for r in rows], backtrack=True)
        if args.guard:
            fell = sum(not guard(c, r) for c, r in zip(bt, rules))
            bt = [c if guard(c, r) else r for c, r in zip(bt, rules)]
            systems[f"backtrack+guard ({fell} fallbacks)"] = bt
        else:
            systems["backtrack"] = bt
        print(f"backtrack changed {sum(a != b for a, b in zip(bt, rules))} of {len(rules)} items", file=sys.stderr)
    for spec in args.text:
        name, path = spec.split("=", 1)
        lines = Path(path).read_text(encoding="utf-8").split("\n")
        lines = (lines + [""] * len(rows))[: len(rows)]
        if args.guard:
            fell = sum(not guard(c, r) for c, r in zip(lines, rules))
            lines = [c if guard(c, r) else r for c, r in zip(lines, rules)]
            name = f"{name}+guard ({fell} fallbacks)"
        systems[name] = lines
    systems["oracle"] = [r["ref"] for r in rows]

    title = "pusmogo-bench outputs" if args.bench else f"polish set ({SET.relative_to(ROOT)})"
    print(table({k: score(v, rows) for k, v in systems.items()}, rows, title))
    if args.show:
        for name, outs in systems.items():
            if name in ("raw", "oracle"):
                continue
            for r, o in zip(rows, outs):
                if o != r["ref"]:
                    print(f"{name}\t{r['id']}\t{o}\t|\t{r['ref']}")


if __name__ == "__main__":
    main()
