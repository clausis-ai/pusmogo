#!/usr/bin/env python3
"""Pick among Whisper's own candidate transcripts with a small language model.

Each input is a `pusmogo-bench --json` result for the same manifests in the
same order, made with different decoding settings (beam search, greedy,
sampling at a few temperatures). For every clip the distinct outputs form the
candidate list, so the picker can never type a word Whisper did not write.

Without `--model` the script prints the baseline (the first JSON) and the
oracle: the best candidate per clip by the bench's own word errors, which is
the most any picker could gain. With `--model` it scores every candidate by
its log-likelihood under a GGUF model (llama-cpp-python, Metal), optionally
plus a weight times the number of runs that produced it (agreement), and
reports the picked result.

Usage:
  scripts/nbest-rescore.py base.json greedy.json t0.3.json ...
  scripts/nbest-rescore.py --model Qwen3-1.7B-Q4_K_M.gguf [--vote W] base.json ...

See docs/bench/polish-07.md.
"""

import argparse
import json
import statistics
import sys
import time
from collections import defaultdict

PREFIX = "Magyar nyelvű diktálás, pontosan leírva:\n"


def load(paths):
    runs = [json.load(open(p))["clips"] for p in paths]
    n = len(runs[0])
    for p, r in zip(paths, runs):
        if len(r) != n or [c["id"] for c in r] != [c["id"] for c in runs[0]]:
            sys.exit(f"{p}: clip list differs from {paths[0]}")
    return runs


def group(clip_id):
    return "fleurs" if clip_id.startswith("fleurs") else "short"


def report(name, picks, runs):
    """picks[i] = run index chosen for clip i."""
    errs = defaultdict(int)
    words = defaultdict(int)
    exact = defaultdict(int)
    for i, r in enumerate(picks):
        c = runs[r][i]
        g = group(c["id"])
        errs[g] += c["wer"]["errors"]
        words[g] += c["wer"]["len"]
        exact[g] += c["wer"]["errors"] == 0
    cells = []
    for g in ("fleurs", "short"):
        if words[g]:
            cells.append(f"{g} WER {100 * errs[g] / words[g]:5.1f} % (exact {exact[g]})")
    print(f"{name:<28} " + " | ".join(cells))


class Scorer:
    def __init__(self, path):
        from llama_cpp import Llama  # scratch venv only, never an app dependency

        global np
        import numpy as np

        self.llm = Llama(model_path=path, n_ctx=1024, n_gpu_layers=-1, logits_all=True, verbose=False)
        self.prefix = self.llm.tokenize(PREFIX.encode(), add_bos=True)

    def logprob(self, text):
        """Sum of log P(token) for the candidate's tokens after the fixed prefix."""
        toks = self.llm.tokenize(text.encode(), add_bos=False, special=False)
        if not toks:
            return -1e9
        allt = self.prefix + toks
        self.llm.reset()
        self.llm.eval(allt)
        p = len(self.prefix)
        rows = np.asarray(self.llm.scores[p - 1:len(allt) - 1], dtype=np.float64)
        m = rows.max(axis=1, keepdims=True)
        lse = (m + np.log(np.exp(rows - m).sum(axis=1, keepdims=True)))[:, 0]
        return float((rows[np.arange(len(toks)), toks] - lse).sum())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model")
    ap.add_argument("--vote", type=float, default=0.0, help="log-prob bonus per run that produced a candidate")
    ap.add_argument("--len-guard", type=float, default=0.0,
                    help="drop candidates whose word count differs from the first run's by more than this fraction")
    ap.add_argument("--dump", help="write per-clip candidates and scores as JSON")
    ap.add_argument("jsons", nargs="+")
    a = ap.parse_args()
    runs = load(a.jsons)
    n = len(runs[0])

    report("baseline (" + a.jsons[0].rsplit("/", 1)[-1] + ")", [0] * n, runs)
    for k in range(1, len(runs)):
        report("run " + a.jsons[k].rsplit("/", 1)[-1], [k] * n, runs)
    oracle = [min(range(len(runs)), key=lambda r: (runs[r][i]["wer"]["errors"], r)) for i in range(n)]
    report("oracle (best candidate)", oracle, runs)
    distinct = [len({runs[r][i]["output"] for r in range(len(runs))}) for i in range(n)]
    print(f"clips with >1 distinct candidate: {sum(d > 1 for d in distinct)}/{n}, mean candidates {statistics.mean(distinct):.2f}")
    votepick = []
    for i in range(n):
        counts = defaultdict(list)
        for r in range(len(runs)):
            counts[runs[r][i]["output"]].append(r)
        best = max(counts.values(), key=lambda rs: (len(rs), -rs[0]))
        votepick.append(best[0])
    report("majority vote", votepick, runs)
    if not a.model:
        return

    sc = Scorer(a.model)
    picks, times, dump = [], [], []
    for i in range(n):
        cands = defaultdict(list)
        for r in range(len(runs)):
            cands[runs[r][i]["output"]].append(r)
        if a.len_guard:
            n0 = len(runs[0][i]["output"].split())
            cands = {t: rs for t, rs in cands.items()
                     if 0 in rs or abs(len(t.split()) - n0) <= a.len_guard * max(n0, 1)}
        t0 = time.perf_counter()
        scored = []
        for text, rs in cands.items():
            lp = sc.logprob(text) if len(cands) > 1 else 0.0
            scored.append((lp + a.vote * len(rs), text, rs))
        times.append(time.perf_counter() - t0)
        best = max(scored, key=lambda s: (s[0], -s[2][0]))
        picks.append(best[2][0])
        dump.append({"id": runs[0][i]["id"], "ref": runs[0][i]["reference"],
                     "cands": [{"text": t, "runs": rs, "score": s} for s, t, rs in scored],
                     "picked": best[1]})
    report(f"LM pick (vote {a.vote:g}, len {a.len_guard:g})", picks, runs)
    multi = [t for t, d in zip(times, distinct) if d > 1]
    if multi:
        q = statistics.quantiles(multi, n=20)
        print(f"scoring time on clips with choices: p50 {statistics.median(multi):.3f} s, p95 {q[18]:.3f} s, max {max(multi):.3f} s")
    changed = sum(p != 0 and runs[p][i]["output"] != runs[0][i]["output"] for i, p in enumerate(picks))
    better = sum(runs[p][i]["wer"]["errors"] < runs[0][i]["wer"]["errors"] for i, p in enumerate(picks))
    worse = sum(runs[p][i]["wer"]["errors"] > runs[0][i]["wer"]["errors"] for i, p in enumerate(picks))
    print(f"picked text differs from baseline on {changed} clips: {better} better, {worse} worse")
    if a.dump:
        json.dump(dump, open(a.dump, "w"), ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
