#!/usr/bin/env python3
"""Head-to-head kit: score dictation outputs on the 30 phrases in bench/compare/phrases.tsv.

Subsets: `hu` (15 everyday Hungarian, h/l/r-initial words, "X vagy Y"
questions), `tech` (10 Hungarian sentences with English tech terms) and `en`
(5 English). Every system is scored by pusmogo-bench, so all of them get the
same normaliser (case, punctuation, numbers) and the same cleanup pass.

Commands:

  tts      Render the phrases with macOS text-to-speech into
           bench/audio/compare/tts/ (a dry run of the kit, no voice needed).
  collect  Copy the dictations Pusmogo kept (Settings: "keep the last 20
           dictations") into bench/audio/compare/voice/<id>.wav, in the order
           they were spoken, and record what the app typed for each. Run it
           after every batch of at most 20 phrases; it continues where the
           last run stopped. --start ID restarts the assignment at ID.
  score    Print one WER/CER table per subset and system:
             pusmogo-app   what the app typed (from the kept records)
             turbo         the recordings replayed through large-v3-turbo q5_0
             large-v3      the recordings replayed through large-v3 q5_0
             NAME          each --text NAME=FILE: one line per phrase, in
                           phrases.tsv order (an empty line = no output)
           --audio tts scores the TTS clips instead of the recordings.

Recordings of a real voice stay under bench/audio/ (git-ignored): never commit them.

Usage:
  scripts/score-compare.py tts
  scripts/score-compare.py collect [--start ID] [--debug-dir DIR]
  scripts/score-compare.py score [--audio voice|tts] [--text NAME=FILE ...] [--no-models]
"""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PHRASES = ROOT / "bench/compare/phrases.tsv"
OUT = ROOT / "bench/audio/compare"
BENCH = ROOT / "core/target/release/pusmogo-bench"
MODELS = Path.home() / "Library/Application Support/Pusmogo/models"
DEBUG_DIR = Path.home() / "Library/Application Support/Pusmogo/debug"
SYSTEMS = [("turbo", "ggml-large-v3-turbo-q5_0.bin"), ("large-v3", "ggml-large-v3-q5_0.bin")]
VOICES = {"hu": "Tünde", "en": "Eddy (English (US))"}
SUBSETS = ["hu", "tech", "en"]
MIN_SECS = 0.5  # shorter kept dictations are stray key taps, not phrases


def phrases() -> list[dict]:
    rows = []
    for line in PHRASES.read_text(encoding="utf-8").splitlines():
        if line.strip() and not line.startswith("#"):
            pid, subset, lang, ref = line.split("\t", 3)
            rows.append({"id": pid, "subset": subset, "lang": lang, "ref": ref})
    return rows


def write_manifest(path: Path, rows: list[dict]) -> None:
    path.write_text("".join(f"{r['id']}\t{r['lang']}\t{r['ref']}\n" for r in rows), encoding="utf-8")


def cmd_tts(_args) -> None:
    out = OUT / "tts"
    out.mkdir(parents=True, exist_ok=True)
    for r in phrases():
        with tempfile.TemporaryDirectory() as tmp:
            aiff = Path(tmp) / "x.aiff"
            subprocess.run(["say", "-v", VOICES[r["lang"]], "-o", str(aiff), r["ref"]], check=True)
            subprocess.run(["afconvert", "-f", "WAVE", "-d", "LEI16@16000", "-c", "1", str(aiff), str(out / f"{r['id']}.wav")], check=True)
    print(f"rendered {len(phrases())} clips into {out.relative_to(ROOT)}")


def cmd_collect(args) -> None:
    out = OUT / "voice"
    out.mkdir(parents=True, exist_ok=True)
    state_path = out / "collected.json"
    state = json.loads(state_path.read_text()) if state_path.exists() else {"last_stem": "", "app": {}}
    rows = phrases()
    ids = [r["id"] for r in rows]
    if args.start:
        if args.start not in ids:
            sys.exit(f"unknown phrase id {args.start}")
        nxt = ids.index(args.start)
    else:
        done = [i for i, pid in enumerate(ids) if (out / f"{pid}.wav").exists()]
        nxt = max(done) + 1 if done else 0
    debug = Path(args.debug_dir)
    stems = sorted(p.stem for p in debug.glob("*.wav") if p.stem > state["last_stem"] and (debug / f"{p.stem}.json").exists())
    if not stems:
        sys.exit(f"no new kept dictations in {debug} (is the Settings switch on?)")
    for stem in stems:
        rec = json.loads((debug / f"{stem}.json").read_text(encoding="utf-8"))
        state["last_stem"] = stem
        if rec.get("audio_secs", 0) < MIN_SECS:
            print(f"skip   {stem}: {rec.get('audio_secs', 0):.2f} s, too short")
            continue
        if nxt >= len(rows):
            print(f"skip   {stem}: all {len(rows)} phrases already have a recording")
            continue
        r = rows[nxt]
        shutil.copyfile(debug / f"{stem}.wav", out / f"{r['id']}.wav")
        state["app"][r["id"]] = rec.get("cleaned", "")
        print(f"{r['id']}   {r['ref']}\n       typed: {rec.get('cleaned', '')}")
        nxt += 1
    state_path.write_text(json.dumps(state, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    have = sum((out / f"{pid}.wav").exists() for pid in ids)
    print(f"{have}/{len(ids)} phrases recorded" + (f"; next: {ids[nxt]} {rows[nxt]['ref']}" if nxt < len(rows) else ""))
    print("If a line above pairs the wrong phrase, rerun with --start ID after re-recording from ID.")


def run_bench(extra: list[str], manifest: Path) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "out.json"
        res = subprocess.run([str(BENCH), "--json", str(out), *extra, str(manifest)], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        if res.returncode != 0:
            sys.exit(f"pusmogo-bench failed:\n{res.stderr}")
        return {c["id"]: c for c in json.loads(out.read_text())["clips"]}


def score_text(outputs: dict[str, str], rows: list[dict]) -> dict:
    """Scores fixed text per phrase through `--hyp-file`; the clips are short silence, only text counts."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        for r in rows:
            with wave.open(str(tmp / f"{r['id']}.wav"), "wb") as w:
                w.setnchannels(1)
                w.setsampwidth(2)
                w.setframerate(16000)
                w.writeframes(b"\0\0" * 1600)
        write_manifest(tmp / "compare.tsv", rows)
        hyp = tmp / "hyp.tsv"
        hyp.write_text("".join(f"{r['id']}\t0\t{outputs.get(r['id'], '').replace(chr(9), ' ')}\n" for r in rows), encoding="utf-8")
        return run_bench(["--hyp-file", str(hyp)], tmp / "compare.tsv")


def decode_text(path: Path) -> str:
    """Read a plain-text outputs file saved by any common editor.

    UTF-8 with or without a BOM and UTF-16 with a BOM (some editors' "Unicode"
    option) are accepted. RTF, TextEdit's default format, is refused with a
    hint, since its markup would otherwise be counted as lines."""
    raw = path.read_bytes()
    if raw.lstrip().startswith(b"{\\rtf"):
        sys.exit(f"{path}: this is RTF, not plain text. In TextEdit choose "
                 "Format > Make Plain Text, then save it again.")
    if raw.startswith((b"\xff\xfe", b"\xfe\xff")):
        return raw.decode("utf-16")
    try:
        return raw.decode("utf-8-sig")
    except UnicodeDecodeError as e:
        sys.exit(f"{path}: not UTF-8 or UTF-16 text ({e}). Save it as plain "
                 "text in UTF-8.")


def read_text_outputs(path: Path, rows: list[dict]) -> dict[str, str]:
    lines = decode_text(path).splitlines()
    while lines and not lines[-1].strip():
        lines.pop()
    every = phrases()
    if len(lines) != len(every):
        sys.exit(f"{path}: {len(lines)} lines, want {len(every)} (one per phrase, in phrases.tsv order)")
    return {r["id"]: line.strip() for r, line in zip(every, lines)}


def table(results: dict[str, dict], rows: list[dict]) -> str:
    subsets = [s for s in SUBSETS if any(r["subset"] == s for r in rows)] + ["all"]
    head = "| system | " + " | ".join(f"{s} WER | {s} CER" for s in subsets) + " |"
    lines = [head, "|---|" + "---:|" * (2 * len(subsets))]
    for name, clips in results.items():
        cells = []
        for s in subsets:
            ids = [r["id"] for r in rows if s == "all" or r["subset"] == s]
            we = sum(clips[i]["wer"]["errors"] for i in ids)
            wl = sum(clips[i]["wer"]["len"] for i in ids)
            ce = sum(clips[i]["cer"]["errors"] for i in ids)
            cl = sum(clips[i]["cer"]["len"] for i in ids)
            cells += [f"{100 * we / max(wl, 1):.1f} %", f"{100 * ce / max(cl, 1):.1f} %"]
        lines.append(f"| {name} | " + " | ".join(cells) + " |")
    counts = ", ".join(f"{s} {sum(r['subset'] == s for r in rows)}" for s in SUBSETS)
    return "\n".join(lines) + f"\n\nPhrases scored: {len(rows)} ({counts}).\n"


def cmd_score(args) -> None:
    if not BENCH.exists():
        sys.exit("build the bench first: cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper")
    audio = OUT / args.audio
    every = phrases()
    recorded = [r for r in every if (audio / f"{r['id']}.wav").exists()]
    if not args.no_models and not recorded:
        sys.exit(f"no clips in {audio.relative_to(ROOT)}: run `collect` (or `tts`) first, or pass --no-models")
    rows = every if args.no_models else recorded
    results: dict[str, dict] = {}
    state_path = audio / "collected.json"
    if state_path.exists():
        app = json.loads(state_path.read_text(encoding="utf-8"))["app"]
        results["pusmogo-app"] = score_text(app, rows)
    if not args.no_models:
        write_manifest(audio / "compare.tsv", rows)
        for name, file in SYSTEMS:
            model = MODELS / file
            if not model.exists():
                print(f"skip {name}: {model} not found", file=sys.stderr)
                continue
            print(f"replaying {len(rows)} clips through {name} ...", file=sys.stderr)
            results[name] = run_bench(["--model", str(model)], audio / "compare.tsv")
    for spec in args.text:
        name, _, file = spec.partition("=")
        if not file:
            sys.exit(f"--text wants NAME=FILE, got {spec}")
        results[name] = score_text(read_text_outputs(Path(file).expanduser(), rows), rows)
    if not results:
        sys.exit("nothing to score")
    print(table(results, rows))
    if args.show:
        for r in rows:
            print(f"{r['id']}  {r['ref']}")
            for name, clips in results.items():
                print(f"  {name:>12}: {clips[r['id']]['output']}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("tts").set_defaults(fn=cmd_tts)
    c = sub.add_parser("collect")
    c.add_argument("--start", help="assign the next recording to this phrase id")
    c.add_argument("--debug-dir", default=str(DEBUG_DIR))
    c.set_defaults(fn=cmd_collect)
    s = sub.add_parser("score")
    s.add_argument("--audio", choices=["voice", "tts"], default="voice")
    s.add_argument("--text", action="append", default=[], metavar="NAME=FILE")
    s.add_argument("--no-models", action="store_true", help="score only text outputs, no replay")
    s.add_argument("--show", action="store_true", help="also print every output next to its reference")
    s.set_defaults(fn=cmd_score)
    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
