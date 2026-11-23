#!/usr/bin/env python3
"""Run a multilingual punctuation tagger as a polish pass over Hungarian dictation.

The tagger is a token classifier (ONNX): for each word it predicts the mark
that follows it (nothing, "." "," "?" "-" ":"). It never changes, adds or
drops a word, so it can only move punctuation. Its input is the rule-cleaned
text (what Pusmogo types today, `pusmogo-mcp clean_text`).

Modes (how the tagger's marks are merged with the text):
  add      keep every mark already there; add a comma after a word that has
           no mark when the tagger predicts one
  commas   keep sentence marks (. ? ! :), drop the existing commas and let the
           tagger place all commas
  full     drop every mark and use the tagger's . , ? : alone, capitalising
           the word after a new full stop or question mark

Numbers with an inner mark (3.14, 10:30, 1,5) are left alone. Writes one line
per item in set order (for `scripts/score-polish.py --text`), the per-item
wall time to OUT.secs, and prints p50/p95 and the process's peak memory.

Needs onnxruntime, tokenizers and numpy (use a scratch venv) and a model
folder holding `onnx/model_quantized.onnx` (or `onnx/model.onnx`),
`tokenizer.json`, `tokenizer_config.json` and `config.json`.

Usage:
  scripts/polish-tagger.py --model DIR --mode add|commas|full --out OUT.txt [--bench FILE.json ...]
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import re
import resource
import statistics
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("score_polish", ROOT / "scripts/score-polish.py")
sp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sp)

CHUNK, OVERLAP = 400, 50
SENTENCE = ".?!"
WORD_MARKS = ".,?!:;"


class Tagger:
    def __init__(self, folder: Path) -> None:
        onnx = folder / "onnx/model_quantized.onnx"
        if not onnx.exists():
            onnx = folder / "onnx/model.onnx"
        opts = ort.SessionOptions()
        opts.intra_op_num_threads = 4
        self.sess = ort.InferenceSession(str(onnx), opts, providers=["CPUExecutionProvider"])
        self.inputs = {i.name for i in self.sess.get_inputs()}
        self.tok = Tokenizer.from_file(str(folder / "tokenizer.json"))
        cfg = json.loads((folder / "config.json").read_text())
        self.labels = {int(k): v for k, v in cfg["id2label"].items()}
        tcfg = json.loads((folder / "tokenizer_config.json").read_text())
        self.cls = self.tok.token_to_id(tcfg["cls_token"])
        self.sep = self.tok.token_to_id(tcfg["sep_token"])

    def marks(self, text: str) -> dict[int, str]:
        """Character end offset of a token -> predicted mark ("0" = none)."""
        enc = self.tok.encode(text, add_special_tokens=False)
        ids, offs = enc.ids, enc.offsets
        out: dict[int, str] = {}
        start = 0
        while start < len(ids):
            end = min(len(ids), start + CHUNK)
            x = np.array([[self.cls] + ids[start:end] + [self.sep]], dtype=np.int64)
            feed = {"input_ids": x}
            if "attention_mask" in self.inputs:
                feed["attention_mask"] = np.ones_like(x)
            logits = self.sess.run(None, feed)[0][0, 1:-1]
            pred = logits.argmax(-1).tolist()
            keep_to = end if end == len(ids) else end - OVERLAP
            for k in range(start, keep_to):
                s, e = offs[k]
                if e > s:
                    out[e] = self.labels[pred[k - start]]
            if end == len(ids):
                break
            start = keep_to
        return out


def split_words(text: str) -> list[list[str]]:
    """[word, trailing marks] pairs; a mark inside a number stays in the word."""
    out = []
    for w in text.split():
        m = re.match(r"^(.*?)([.,?!:;]*)$", w)
        core, tail = m.group(1), m.group(2)
        if not core:  # a lone mark: attach to the previous word
            if out:
                out[-1][1] += tail
            continue
        out.append([core, tail])
    return out


def polish(tagger: Tagger, text: str, mode: str) -> str:
    words = split_words(text)
    if not words:
        return text
    bare = " ".join(w for w, _ in words)
    marks = tagger.marks(bare)
    pos, pred = 0, []
    for w, _ in words:
        pos += len(w)
        lab = marks.get(pos, "0")
        pred.append("" if lab in ("0", "-") else lab)
        pos += 1
    out = []
    cap_next = False
    for (w, tail), p in zip(words, pred):
        if mode == "add":
            new = tail if tail else ("," if p == "," else "")
        elif mode == "commas":
            kept = tail.replace(",", "")
            new = kept if kept else ("," if p == "," else "")
        else:  # full
            new = p
            if cap_next and w[:1].islower():
                w = w[0].upper() + w[1:]
        cap_next = mode == "full" and bool(new) and new[-1] in SENTENCE
        out.append(w + new)
    last = out[-1]
    if mode == "full" and last[-1:] not in SENTENCE:
        out[-1] = last.rstrip(",:;") + "."
    return " ".join(out)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--model", required=True, type=Path)
    ap.add_argument("--mode", required=True, choices=["add", "commas", "full"])
    ap.add_argument("--out", required=True)
    ap.add_argument("--bench", action="append", default=[], metavar="FILE.json")
    args = ap.parse_args()

    rows = sp.read_bench(args.bench) if args.bench else sp.read_set()
    inputs = sp.clean_all([r["input"] for r in rows])
    t0 = time.perf_counter()
    tagger = Tagger(args.model)
    load = time.perf_counter() - t0
    polish(tagger, "szia hogy vagy", args.mode)  # warm up
    outs, secs = [], []
    for text in inputs:
        t0 = time.perf_counter()
        outs.append(polish(tagger, text, args.mode))
        secs.append(time.perf_counter() - t0)
    Path(args.out).write_text("\n".join(outs) + "\n", encoding="utf-8")
    Path(args.out + ".secs").write_text("\n".join(f"{s:.4f}" for s in secs) + "\n")
    q = statistics.quantiles(secs, n=20)
    peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1e6  # bytes on macOS
    print(f"n={len(secs)} load={load:.2f}s p50={statistics.median(secs):.3f}s "
          f"p95={q[18]:.3f}s max={max(secs):.3f}s peak_rss={peak:.0f}MB")


if __name__ == "__main__":
    main()
