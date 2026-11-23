# bakeoff-02: large-v3 vs turbo on short Hungarian phrases

Up: [../README.md](../README.md)
Verified: 2026-11-08

Short Hungarian dictations (one to four words) are where Pusmogo is weakest: 35 % WER against 17 % on read sentences ([short-01](short-01.md), [beam-01](beam-01.md)). [bakeoff-01](bakeoff-01.md) showed large-v3 q5_0 is more accurate than the shipped turbo q5_0 on FLEURS sentences. This report asks whether the gap is larger on the short phrases. Recommendation only; the default model does not change.

## Result

| set | clips | turbo q5_0 WER % | large-v3 q5_0 WER % | turbo CER % | large-v3 CER % | turbo p50 / p95 s | large-v3 p50 / p95 s |
|---|---|---|---|---|---|---|---|
| short hu | 44 | 31.5 | **18.5** | 9.6 | **4.9** | 0.60 / 0.61 | 0.73 / 0.79 |
| short low (quiet) | 44 | 25.0 | **13.0** | 7.4 | **3.8** | 0.59 / 0.61 | 0.73 / 0.79 |
| short noisy | 44 | 50.0 | **40.2** | 25.9 | **19.7** | 0.60 / 0.62 | 0.75 / 0.79 |
| short, all | 132 | 35.5 | **23.9** | 14.3 | **9.5** | 0.60 / 0.61 | 0.73 / 0.79 |

Apple M5 Pro with Metal, release `pusmogo-bench --features whisper`, current decoding (beam 5 under 20 s, Hungarian carrier prompt, Silero speech detector on for turbo; for large-v3 the detector file was missing from its folder, but a rerun with it gave identical outputs, see [bakeoff-03](bakeoff-03.md)), same manifests in the same order. Load 0.17 s vs 0.30 s. Peak memory from bakeoff-01: 755 MB vs 1553 MB. Synthetic speech (macOS TTS, Tünde) is a proxy for a real voice.

## Findings

1. **large-v3 cuts short-phrase word errors by a third** (35.5 → 23.9 %), a bigger relative gain than on FLEURS sentences (17.4 → 14.1 %). Exact outputs: 69 → 89 of 132. Per clip: 35 better, 8 worse, 89 the same. Empty outputs: 1 → 1.
2. **It fixes the error class seen in the first real-voice test.** "Mi a lopás versus rablás?" was "Mi a lopás versúsra blás?" with turbo and is exact with large-v3 (clean and quiet). Single words that turbo respelled the English way are now right: "Lopash" → "Lopás", "Ruka" → "Róka", "Líst" → "Liszt", "Regál" → "Reggel", "Hét fő" → "Hétfő". In noise: "RUKA" → "Róka", "Levil" → "Levél.", an empty clip → "Rossz a hang.", "Kérek egy kádi!" → "Kérek egy kávét."
3. **What got worse.** Word joins and endings: "Levelet írok." → "Leveletírók" (twice), "mondok" → "mondók", "Rablás" → "Roblás" (quiet). Over-completion of a greeting: "Szia" → "Sziasztok!" (also in noise, where turbo had "Szia, hogy vagy?" right). "Hol a lámpa?" → "Hulló lámpa!" in fan noise.
4. **Cost.** About 0.13 s more per short pass (p50 0.60 → 0.73 s), twice the memory and a 1.08 GB download instead of 574 MB. The noise guards in `vad.rs` (confidence floors, known phrases) were tuned on turbo; they were not re-tuned here.

## What it means

The model, not decoding or prompts, is the biggest remaining lever on short Hungarian phrases. This strengthens the open decision to offer large-v3 as an optional "most accurate" model. Before shipping it as an option, re-run the noise and silence sets with it (the guards were tuned on turbo) and time the release wait in streaming mode.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper && cd ..
# large-v3 q5_0: public ggml file from ggerganov/whisper.cpp on Hugging Face (MIT),
# sha256 d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1; keep it outside the repo
core/target/release/pusmogo-bench --model <model.bin> --json out.json --md out.md \
  bench/audio/short/hu.tsv bench/audio/short/low.tsv bench/audio/short/noisy.tsv
```
