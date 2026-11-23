#!/usr/bin/env bash
# Try two speech models on one clip of your own voice, side by side.
#
#   scripts/try-models.sh            # record from the default microphone until Enter
#   scripts/try-models.sh clip.wav   # or transcribe an existing WAV (any rate, any channels)
#   scripts/try-models.sh --en ...   # English instead of Hungarian (the language is always forced)
#   scripts/try-models.sh --keep ... # keep the 16 kHz recording and print where it is
#
# Prints Whisper large-v3 (q5_0, through pusmogo-bench, with the app's cleanup) and
# canary-1b-v2 at 4-bit (MLX, raw model output) with the wall time of each.
# Bench only: nothing here ships in the app, and nothing is saved unless --keep.
#
# Needs:
#   - core/target/release/pusmogo-bench built with Whisper:
#       (cd core && cargo build --release -j 4 -p pusmogo-bench --features whisper)
#   - ~/Library/Application Support/Pusmogo/models/ggml-large-v3-q5_0.bin
#   - a Python venv with mlx-audio (default /tmp/m2/.venv; override with TRY_VENV):
#       uv venv -p 3.12 /tmp/m2/.venv && uv pip install --python /tmp/m2/.venv/bin/python mlx-audio==0.6.4
#   - the canary weights in the Hugging Face cache under .../Pusmogo/m2/hf (override with HF_HOME):
#       HF_HOME=".../Pusmogo/m2/hf" hf download TechHara/canary-1b-v2-mlx-q4
#       HF_HOME=".../Pusmogo/m2/hf" hf download eelcor/canary-1b-v2-mlx tokenizer.model
#   - ffmpeg (Homebrew) for recording and resampling.
# See docs/bench/models-02.md for how these models were measured.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_SUPPORT="$HOME/Library/Application Support/Pusmogo"
LANG_CODE=hu
KEEP=0
INPUT=""
for a in "$@"; do
  case "$a" in
    --en) LANG_CODE=en ;;
    --hu) LANG_CODE=hu ;;
    --keep) KEEP=1 ;;
    -h|--help) sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option: $a" >&2; exit 2 ;;
    *) INPUT="$a" ;;
  esac
done

BENCH="$ROOT/core/target/release/pusmogo-bench"
WHISPER_MODEL="${PUSMOGO_MODEL:-$APP_SUPPORT/models/ggml-large-v3-q5_0.bin}"
VENV="${TRY_VENV:-/tmp/m2/.venv}"
export HF_HOME="${HF_HOME:-$APP_SUPPORT/m2/hf}"

fail() { echo "try-models: $*" >&2; exit 1; }
command -v ffmpeg >/dev/null || fail "ffmpeg not found (brew install ffmpeg)"
[ -x "$BENCH" ] || fail "missing $BENCH (see the build line at the top of this script)"
[ -f "$WHISPER_MODEL" ] || fail "missing Whisper model $WHISPER_MODEL"
[ -x "$VENV/bin/python" ] || fail "missing venv $VENV (see the uv lines at the top of this script)"
snap() { ls -d "$HF_HOME/hub/models--$1/snapshots/"*/ 2>/dev/null | head -1; }
Q4="$(snap TechHara--canary-1b-v2-mlx-q4)"
TOK="$(snap eelcor--canary-1b-v2-mlx)"
[ -n "$Q4" ] && [ -f "$TOK/tokenizer.model" ] || fail "canary weights not found under $HF_HOME (see the download lines at the top of this script)"

WORK="$(mktemp -d -t pusmogo-try)"
trap 'rm -rf "$WORK"' EXIT
WAV="$WORK/clip.wav"

if [ -n "$INPUT" ]; then
  [ -f "$INPUT" ] || fail "no such file: $INPUT"
  ffmpeg -hide_banner -loglevel error -i "$INPUT" -ac 1 -ar 16000 -sample_fmt s16 "$WAV"
else
  echo "Recording from the default microphone. Speak, then press Enter."
  ffmpeg -hide_banner -loglevel error -f avfoundation -i ":default" -ac 1 -ar 16000 -sample_fmt s16 "$WAV" </dev/null &
  REC=$!
  read -r _
  kill -INT "$REC" 2>/dev/null || true
  wait "$REC" 2>/dev/null || true
  [ -s "$WAV" ] || fail "nothing was recorded (microphone permission for this terminal?)"
fi
SECS="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$WAV")"
printf 'clip: %.1f s, language %s\n\n' "$SECS" "$LANG_CODE"

# Whisper large-v3 through the bench, as the app runs it (the reference column is a placeholder).
printf 'clip\t%s\t-\n' "$LANG_CODE" > "$WORK/one.tsv"
t0=$(python3 -c 'import time; print(time.time())')
PUSMOGO_MODEL="$WHISPER_MODEL" "$BENCH" --json "$WORK/w.json" "$WORK/one.tsv" >/dev/null 2>"$WORK/w.log" \
  || { cat "$WORK/w.log" >&2; fail "pusmogo-bench failed"; }
t1=$(python3 -c 'import time; print(time.time())')
python3 - "$WORK/w.json" "$t0" "$t1" <<'PY'
import json, sys
j = json.load(open(sys.argv[1]))
if j["engine"] == "stub":
    sys.exit("try-models: pusmogo-bench was built without --features whisper")
c = j["clips"][0]
wall = float(sys.argv[3]) - float(sys.argv[2])
print(f"large-v3 q5_0   transcribe {c['latency_s']:.2f} s  (wall incl. load {wall:.1f} s)")
print(f"  {c['output']}\n")
PY

# canary-1b-v2 4-bit through mlx-audio, language forced, cache limit 0 as measured in models-02.
mkdir "$WORK/canary"
for f in config.json model.safetensors tokens.json; do ln -s "$Q4$f" "$WORK/canary/$f"; done
ln -s "$TOK/tokenizer.model" "$WORK/canary/tokenizer.model"
cat > "$WORK/canary.py" <<'PY'
import sys, time, wave
import mlx.core as mx
import numpy as np
from mlx_audio.stt import load
model_dir, wav, lang = sys.argv[1:]
mx.set_cache_limit(0)
t = time.perf_counter()
model = load(model_dir)
load_s = time.perf_counter() - t
with wave.open(wav) as w:
    a = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float32) / 32768.0
audio = mx.array(a)
model.generate(audio[:16000], language=lang)  # warm-up, not timed
t = time.perf_counter()
r = model.generate(audio, language=lang)
dt = time.perf_counter() - t
print(f"canary-1b-v2 q4 transcribe {dt:.2f} s  (load {load_s:.1f} s, raw output, no cleanup)")
print("  " + " ".join(r.text.split()) + "\n")
PY
(cd "$WORK" && "$VENV/bin/python" -I "$WORK/canary.py" "$WORK/canary" "$WAV" "$LANG_CODE") 2>"$WORK/c.log" \
  || { tail -20 "$WORK/c.log" >&2; fail "canary run failed"; }

if [ "$KEEP" = 1 ]; then
  OUT="$ROOT/bench/audio/try-$(date +%Y%m%d-%H%M%S).wav"
  cp "$WAV" "$OUT"
  echo "kept: $OUT (git-ignored; never commit recordings of real people)"
fi
