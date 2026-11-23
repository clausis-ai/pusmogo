#!/usr/bin/env bash
# Run one Whisper ggml model over the fine-tune gate sets and keep JSON + markdown per set.
#
#   scripts/bench-finetune.sh <model.bin> <name> <out-dir> <whisper-sets-dir>
#
# Sets: FLEURS Hungarian and English (real speech), the short Hungarian phrases
# (normal, low volume, noisy), the synthetic sets (Hungarian, mixed, English,
# silence, noise, speech over noise) and the pseudo-whispered FLEURS and short
# sets (scripts/pseudo-whisper.py output, outside the repository).
# Run models one after another on an idle machine; latency is only comparable then.
set -euo pipefail
model=${1:?model}; name=${2:?name}; out=${3:?out dir}; pw=${4:?whispered sets dir}
root=$(cd "$(dirname "$0")/.." && pwd)
bench=${PUSMOGO_BENCH:-$root/core/target/release/pusmogo-bench}
a=${PUSMOGO_AUDIO:-$root/bench/audio}
mkdir -p "$out"
run() { "$bench" --model "$model" --json "$out/$name-$1.json" --md "$out/$name-$1.md" "${@:2}" >/dev/null 2>&1; echo "$name $1 done"; }
run fleurs-hu "$a/fleurs-hu/hu.tsv"
run fleurs-en "$a/fleurs-en/en.tsv"
run short "$a/short/hu.tsv" "$a/short/low.tsv" "$a/short/noisy.tsv"
run synth "$a"/synth/{hu,mixed,en,silence,noise,noisy}.tsv
run whisper "$pw/whisper-fleurs-hu/fleurs.tsv" "$pw/whisper-short-hu/short.tsv"
