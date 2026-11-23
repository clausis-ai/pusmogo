#!/usr/bin/env python3
"""Turn VoxPopuli parquet files into a training manifest in the bench format.

VoxPopuli (data CC0; raw audio also under the European Parliament legal
notice) ships Hugging Face parquet files whose rows hold the clip as WAV bytes,
the cased and punctuated transcript (raw_text) and a speaker id. This writes
each clip as 16 kHz mono 16-bit WAV and a manifest
`id<TAB>lang<TAB>reference<TAB>speaker` that data-audit.py and
make-finetune-data.py read. Use the train and validation files only; the test
file stays out of training.

Needs pyarrow (run with: uv run --with pyarrow scripts/import-voxpopuli.py ...).
Clips that are not already 16 kHz mono 16-bit go through afconvert.

Usage:
  scripts/import-voxpopuli.py <out-dir> <file.parquet>... [--lang hu]
"""

import argparse
import io
import subprocess
import sys
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    import pyarrow.parquet as pq

    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("out", type=Path)
    p.add_argument("parquet", type=Path, nargs="+")
    p.add_argument("--lang", default="hu")
    a = p.parse_args()
    out = a.out.resolve()
    if out == ROOT or ROOT in out.parents:
        p.error("write the clips outside the repository (recordings of real people)")
    if any("test" in f.name for f in a.parquet):
        p.error("the test split never trains")
    out.mkdir(parents=True, exist_ok=True)

    lines, converted, skipped = [], 0, 0
    for path in a.parquet:
        pf = pq.ParquetFile(path)
        for rg in range(pf.num_row_groups):
            for row in pf.read_row_group(rg, columns=["audio_id", "audio", "raw_text", "speaker_id"]).to_pylist():
                text = (row["raw_text"] or "").replace("\t", " ").strip()
                data = (row["audio"] or {}).get("bytes")
                if not data:
                    skipped += 1
                    continue
                cid = "vp-" + row["audio_id"].replace(":", "-")
                dest = out / f"{cid}.wav"
                if not dest.exists():
                    try:
                        with wave.open(io.BytesIO(data)) as w:
                            ok = (w.getframerate(), w.getnchannels(), w.getsampwidth()) == (16000, 1, 2)
                    except wave.Error:
                        ok = False
                    if ok:
                        dest.write_bytes(data)
                    else:
                        raw = out / "tmp-in.wav"
                        raw.write_bytes(data)
                        subprocess.run(["afconvert", "-f", "WAVE", "-d", "LEI16@16000", "-c", "1", str(raw), str(dest)], check=True)
                        raw.unlink()
                        converted += 1
                lines.append(f"{cid}.wav\t{a.lang}\t{text}\t{row['speaker_id'] or ''}")
        print(f"{path.name}: {len(lines)} clips so far", file=sys.stderr)

    manifest = out / f"{a.lang}.tsv"
    manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {len(lines)} clips ({converted} converted, {skipped} without audio) and {manifest}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
