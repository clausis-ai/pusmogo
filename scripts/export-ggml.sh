#!/usr/bin/env bash
# Convert a fine-tuned Hugging Face Whisper checkpoint into the q5_0 ggml file the app loads.
#
#   scripts/export-ggml.sh <hf-model-dir> <out.bin> [base-model-id]
#
# Needs a whisper.cpp checkout with whisper-quantize built (WHISPER_CPP, MIT) and
# openai/whisper (OPENAI_WHISPER, MIT, for the mel filters the converter reads),
# plus uv for the converter's Python. Newer transformers save only tokenizer.json,
# while the converter reads vocab.json and added_tokens.json, so those are taken
# from the base model (the fine-tune does not change the tokenizer).
set -euo pipefail
src=${1:?hf model dir}; out=${2:?output .bin}; base=${3:-openai/whisper-large-v3-turbo}
cpp=${WHISPER_CPP:?set WHISPER_CPP to a whisper.cpp checkout}
oai=${OPENAI_WHISPER:?set OPENAI_WHISPER to an openai/whisper checkout}
quant="$cpp/build/bin/whisper-quantize"
[[ -x "$quant" ]] || { echo "build it first: cmake -B build && cmake --build build --target whisper-quantize (in $cpp)"; exit 1; }
for f in vocab.json added_tokens.json; do
  [[ -f "$src/$f" ]] || curl -sSfL -o "$src/$f" "https://huggingface.co/$base/resolve/main/$f"
done
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
uv run -q --python 3.12 --with torch --with transformers --with numpy \
  python -I "$cpp/models/convert-h5-to-ggml.py" "$src" "$oai" "$tmp" >/dev/null
"$quant" "$tmp/ggml-model.bin" "$out" q5_0 >/dev/null
ls -l "$out"
