#!/usr/bin/env python3
"""Audit a speech training set before it reaches make-finetune-data.py.

A fine-tune learns whatever its data says, mistakes included, so every clip
passes a set of checks first. Each check keeps, fixes, drops or flags a clip
for a human to hear, and every decision is written down with its reason:

  format         not 16 kHz mono 16-bit WAV                         drop
  duration       shorter than --min-s or longer than --max-s       drop
  clipped        more than 0.1 % of samples at full scale           drop
  quiet          speech level (90th percentile of 30 ms frame RMS)
                 below --min-dbfs (silence, wrong file)              drop
  low-level      speech level below --review-dbfs (very quiet take)   review
  empty-text     no reference text                                  drop
  latin1-accent  o/u with tilde or circumflex where Hungarian has
                 the double acute (õ -> ő, û -> ű)                   fix
  final-stop     reference ends without . ! ? or …                 fix (adds .)
  odd-char       a character outside the language's letters,
                 digits and common punctuation                      review
  roman-numeral  a Roman numeral the app never types (XVI.)         review
  duplicate      same speaker, same sentence                        drop
  asr-mismatch   the best --asr run is further than --max-cer from
                 the reference (wrong audio or wrong text)          drop
  asr-doubt      between --review-cer and --max-cer                 review
  speaker-cap    more than --speaker-cap clips from one speaker     drop

--asr takes pusmogo-bench --json runs over the same manifests; with several
runs a clip counts by its lowest CER, so one model hearing it right is enough.
Without --asr the audio-text check is skipped and the report says so.

Outputs:
  next to each manifest: <name>.keep.tsv, the clips that passed (fixed text),
  in the same format, ready for make-finetune-data.py --source
  <out>/audit.json   counts and hours per reason and source, speakers,
                     licences, settings
  <out>/dropped.tsv, review.tsv  source, id, reason, detail, reference
  <out>/listen.tsv   a fixed random sample of kept clips for a listening check

Usage:
  scripts/data-audit.py <out-dir> --source <manifest.tsv>... [--asr <bench.json>...]
      [--licence "FLEURS: CC-BY-4.0"] [--min-s 1] [--max-s 30] [--min-dbfs -60] [--review-dbfs -45]
      [--max-cer 0.5] [--review-cer 0.2] [--speaker-cap 400] [--listen 200] [--seed 1]
"""

import argparse
import array
import hashlib
import json
import math
import re
import sys
import unicodedata
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

LETTERS = {
    "hu": "aábcdeéfghiíjklmnoóöőpqrstuúüűvwxyz",
    "en": "abcdefghijklmnopqrstuvwxyz",
}
PUNCT = set(" .,!?;:'\"„”“«»()[]-–—%/&+°…’")
LATIN1 = str.maketrans({"õ": "ő", "ô": "ő", "Õ": "Ő", "Ô": "Ő", "û": "ű", "ũ": "ű", "Û": "Ű", "Ũ": "Ű"})
ROMAN = re.compile(r"\b(?=[IVXLCDM]{2,}\b)M{0,3}(CM|CD|D?C{0,3})(XC|XL|L?X{0,3})(IX|IV|V?I{0,3})\b\.?")
FINAL = (".", "!", "?", "…")


def unit(key: str, seed: int) -> float:
    digest = hashlib.sha256(f"{seed}:{key}".encode()).digest()
    return int.from_bytes(digest[:8], "little") / 2**64


def norm(text: str) -> str:
    text = unicodedata.normalize("NFC", text).casefold()
    text = re.sub(r"[^\w\s]", " ", text)
    return " ".join(text.split())


def audio_facts(path: Path) -> dict:
    with wave.open(str(path), "rb") as w:
        rate, chans, width, n = w.getframerate(), w.getnchannels(), w.getsampwidth(), w.getnframes()
        raw = w.readframes(n)
    facts = {"rate": rate, "channels": chans, "width": width, "seconds": n / rate if rate else 0.0}
    if (rate, chans, width) != (16000, 1, 2) or n == 0:
        return facts
    x = array.array("h")
    x.frombytes(raw)
    if sys.byteorder == "big":
        x.byteswap()
    peak = sum(1 for v in x if v >= 32700 or v <= -32700)
    frames = sorted(math.sqrt(sum(v * v for v in x[i:i + 480]) / 480) for i in range(0, len(x) - 479, 480)) or [0.0]
    loud = frames[int(0.9 * (len(frames) - 1))] or 1e-9
    facts["clipped_share"] = peak / len(x)
    facts["dbfs"] = 20 * math.log10(loud / 32768)
    return facts


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
        rows.append({
            "manifest": path, "cols": cols, "id": Path(wav.name).stem, "wav": wav,
            "lang": cols[1], "text": unicodedata.normalize("NFC", cols[2].strip()),
            "speaker": cols[3].strip() if len(cols) > 3 and cols[3].strip() else None,
            "source": path.parent.name,
        })
    return rows


def load_asr(paths: list[Path]) -> dict:
    best: dict[str, float] = {}
    for p in paths:
        for c in json.loads(p.read_text(encoding="utf-8"))["clips"]:
            key = Path(str(c["id"])).stem
            rate = c["cer"]["rate"]
            best[key] = min(rate, best.get(key, rate))
    return best


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("out", type=Path)
    p.add_argument("--source", type=Path, nargs="+", required=True)
    p.add_argument("--asr", type=Path, nargs="*", default=[])
    p.add_argument("--licence", action="append", default=[])
    p.add_argument("--min-s", type=float, default=1.0)
    p.add_argument("--max-s", type=float, default=30.0)
    p.add_argument("--min-dbfs", type=float, default=-60.0)
    p.add_argument("--review-dbfs", type=float, default=-45.0)
    p.add_argument("--max-cer", type=float, default=0.5)
    p.add_argument("--review-cer", type=float, default=0.2)
    p.add_argument("--speaker-cap", type=int, default=400)
    p.add_argument("--listen", type=int, default=200)
    p.add_argument("--seed", type=int, default=1)
    a = p.parse_args()

    out = a.out.resolve()
    if out == ROOT or ROOT in out.parents:
        p.error("write the audit outside the repository (it quotes recordings of real people)")
    out.mkdir(parents=True, exist_ok=True)
    asr = load_asr(a.asr)

    rows = [r for m in a.source for r in read_manifest(m)]
    dropped, review, fixed = [], [], {"latin1-accent": 0, "final-stop": 0}

    def drop(r, reason, detail=""):
        r["dropped"] = reason
        dropped.append((r, reason, detail))

    seen = set()
    for i, r in enumerate(rows, 1):
        if i % 500 == 0:
            print(f"checked {i}/{len(rows)}", file=sys.stderr)
        if not r["wav"].exists():
            drop(r, "missing-audio")
            continue
        f = audio_facts(r["wav"])
        r["seconds"] = f["seconds"]
        if (f["rate"], f["channels"], f["width"]) != (16000, 1, 2):
            drop(r, "format", f"{f['rate']} Hz, {f['channels']} ch, {8 * f['width']} bit")
            continue
        if not a.min_s <= f["seconds"] <= a.max_s:
            drop(r, "duration", f"{f['seconds']:.2f} s")
            continue
        if f["clipped_share"] > 0.001:
            drop(r, "clipped", f"{100 * f['clipped_share']:.2f} %")
            continue
        if f["dbfs"] < a.min_dbfs:
            drop(r, "quiet", f"{f['dbfs']:.1f} dBFS")
            continue
        if f["dbfs"] < a.review_dbfs:
            review.append((r, "low-level", f"{f['dbfs']:.1f} dBFS"))
        text = r["text"]
        if not text:
            drop(r, "empty-text")
            continue
        fixed_text = text.translate(LATIN1)
        if fixed_text != text:
            fixed["latin1-accent"] += 1
            text = fixed_text
        if not text.endswith(FINAL) and text[-1:].isalnum():
            fixed["final-stop"] += 1
            text += "."
        r["text"] = text
        key = (r["speaker"], norm(text))
        if r["speaker"] and key in seen:
            drop(r, "duplicate")
            continue
        seen.add(key)
        if r["id"] in asr:
            cer = asr[r["id"]]
            r["cer"] = cer
            if cer > a.max_cer:
                drop(r, "asr-mismatch", f"CER {cer:.2f}")
                continue
            if cer > a.review_cer:
                review.append((r, "asr-doubt", f"CER {cer:.2f}"))
        letters = LETTERS.get(r["lang"])
        if letters:
            odd = sorted({c for c in text.casefold() if c not in letters and not c.isdigit() and c not in PUNCT})
            if odd:
                review.append((r, "odd-char", "".join(odd)))
        if ROMAN.search(text):
            review.append((r, "roman-numeral", ROMAN.search(text).group(0)))

    by_speaker: dict[str, list] = {}
    for r in rows:
        if "dropped" not in r and r["speaker"]:
            by_speaker.setdefault(r["speaker"], []).append(r)
    for spk, clips in by_speaker.items():
        if len(clips) > a.speaker_cap > 0:
            clips.sort(key=lambda r: unit(r["id"], a.seed))
            for r in clips[a.speaker_cap:]:
                drop(r, "speaker-cap", spk)

    kept = [r for r in rows if "dropped" not in r]
    for m in a.source:
        lines = ["\t".join([r["cols"][0], r["lang"], r["text"], *r["cols"][3:]])
                 for r in kept if r["manifest"] == m]
        m.with_suffix(".keep.tsv").write_text("\n".join(lines) + ("\n" if lines else ""), encoding="utf-8")

    def tsv(name, items):
        with open(out / name, "w", encoding="utf-8") as fh:
            fh.write("source\tid\treason\tdetail\treference\n")
            for r, reason, detail in items:
                fh.write(f"{r['source']}\t{r['id']}\t{reason}\t{detail}\t{r['text']}\n")

    tsv("dropped.tsv", dropped)
    tsv("review.tsv", review)
    sample = sorted(kept, key=lambda r: unit("listen:" + r["id"], a.seed))[: a.listen]
    with open(out / "listen.tsv", "w", encoding="utf-8") as fh:
        fh.write("source\tid\twav\treference\tok (y/n)\n")
        for r in sample:
            fh.write(f"{r['source']}\t{r['id']}\t{r['wav']}\t{r['text']}\t\n")

    hours = lambda rs: round(sum(r.get("seconds", 0.0) for r in rs) / 3600, 3)
    reasons: dict[str, list] = {}
    for r, reason, _ in dropped:
        reasons.setdefault(reason, []).append(r)
    flagged: dict[str, int] = {}
    for _, reason, _ in review:
        flagged[reason] = flagged.get(reason, 0) + 1
    speakers = {r["speaker"] for r in kept if r["speaker"]}
    top = max((len([r for r in kept if r["speaker"] == s]) for s in speakers), default=0)
    report = {
        "sources": [str(m) for m in a.source],
        "asr_runs": [str(x) for x in a.asr],
        "asr_check": "on" if a.asr else "SKIPPED: no --asr run given",
        "asr_coverage": round(sum(1 for r in rows if r["id"] in asr) / len(rows), 3) if rows else 0,
        "licences": a.licence,
        "settings": {k: getattr(a, k) for k in ("min_s", "max_s", "min_dbfs", "review_dbfs", "max_cer", "review_cer", "speaker_cap", "seed")},
        "clips": {"in": len(rows), "kept": len(kept), "dropped": len(dropped), "review": len(review)},
        "hours": {"in": hours(rows), "kept": hours(kept)},
        "dropped_by_reason": {k: {"clips": len(v), "hours": hours(v)} for k, v in sorted(reasons.items())},
        "review_by_reason": dict(sorted(flagged.items())),
        "fixed": fixed,
        "speakers": {"kept": len(speakers), "largest_share": round(top / len(kept), 4) if kept else 0},
    }
    (out / "audit.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report["clips"]), json.dumps(report["dropped_by_reason"]), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
