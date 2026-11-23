#!/usr/bin/env python3
"""Run a small local language model as a text polish pass over the polish set.

The model sits behind a llama.cpp server (`llama-server`, OpenAI-style chat
endpoint). For each item the script sends the rule-cleaned text (what Pusmogo
types today) with a strict edit-only instruction and writes the answer, one
line per item in set order, ready for `scripts/score-polish.py --text`.
Per-item wall time goes to a second file and p50/p95 are printed.

Start a server first, for example:
  llama-server -m MODEL.gguf --port 8090 -ngl 99 -c 2048 --jinja \
      --spec-type ngram-simple --spec-ngram-simple-size-n 3 --spec-ngram-simple-size-m 24

Usage:
  scripts/polish-llm.py --out OUT.txt [--bench FILE.json ...] [--port 8090]
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import statistics
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("score_polish", ROOT / "scripts/score-polish.py")
sp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sp)

SYSTEM = (
    "Magyar diktált szöveget javítasz. Szabályok:\n"
    "1. Csak írásjelet (vessző, pont, kérdőjel) és kis- vagy nagybetűt javíthatsz.\n"
    "2. Ha a beszélő menet közben kijavítja magát (például \"háromkor, nem, négykor\"), "
    "csak a javított változat maradjon.\n"
    "3. Más szót soha ne adj hozzá, ne hagyj el és ne cserélj ki. A számokat és a helyesírást ne változtasd.\n"
    "4. Ha a szöveg már jó, add vissza változatlanul.\n"
    "Csak a javított szöveget írd ki, magyarázat nélkül."
)

# Worked examples, deliberately different from every item of bench/polish/hu.tsv.
SHOTS = [
    ("Szerintem jó lesz ha kedden beszélünk.", "Szerintem jó lesz, ha kedden beszélünk."),
    ("Küldd el a fájlt Gábornak, nem, Ritának.", "Küldd el a fájlt Ritának."),
    ("A projekt jól halad, a teszt is lefutott.", "A projekt jól halad, a teszt is lefutott."),
    ("Vegyél kenyeret tejet és sajtot.", "Vegyél kenyeret, tejet és sajtot."),
]


def ask(port: int, text: str, max_tokens: int) -> str:
    msgs = [{"role": "system", "content": SYSTEM}]
    for q, a in SHOTS:
        msgs += [{"role": "user", "content": q}, {"role": "assistant", "content": a}]
    msgs.append({"role": "user", "content": text})
    body = {
        "messages": msgs,
        "temperature": 0,
        "max_tokens": max_tokens,
        "chat_template_kwargs": {"enable_thinking": False},
    }
    req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",
                                 data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        out = json.load(r)["choices"][0]["message"]["content"]
    # Qwen3 may still emit an empty think block; keep the last non-empty line.
    if "</think>" in out:
        out = out.split("</think>", 1)[1]
    lines = [ln.strip() for ln in out.strip().splitlines() if ln.strip()]
    return lines[-1] if lines else ""


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--out", required=True)
    ap.add_argument("--bench", action="append", default=[], metavar="FILE.json")
    ap.add_argument("--port", type=int, default=8090)
    args = ap.parse_args()

    rows = sp.read_bench(args.bench) if args.bench else sp.read_set()
    inputs = sp.clean_all([r["input"] for r in rows])
    ask(args.port, inputs[0], 8)  # warm up
    outs, secs = [], []
    for text in inputs:
        t0 = time.perf_counter()
        out = ask(args.port, text, max_tokens=2 * len(text.split()) * 4 + 16)
        secs.append(time.perf_counter() - t0)
        outs.append(out.replace("\n", " "))
    Path(args.out).write_text("\n".join(outs) + "\n", encoding="utf-8")
    Path(args.out + ".secs").write_text("\n".join(f"{s:.3f}" for s in secs) + "\n")
    q = statistics.quantiles(secs, n=20)
    print(f"n={len(secs)} p50={statistics.median(secs):.3f}s p95={q[18]:.3f}s max={max(secs):.3f}s")


if __name__ == "__main__":
    main()
