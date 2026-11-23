# Bench 01: pusmogo-bench on the E2E TTS clips (2026-11-07)

First run of `pusmogo-bench` (A1) with the real model, `--features whisper`, Apple Silicon. 8 macOS TTS clips (Tünde / Eddy): **TTS is a proxy, not real speech**, and 8 clips is a smoke test, not a benchmark. Scores use the bench normaliser (lowercase, no punctuation, number words → digits, % → word), so "huszonhét"/"27" and "forty percent"/"40%" count as correct. Latency = engine + cleanup, warm engine.

Engine `whisper` (`ggml-large-v3-turbo-q5_0.bin`), load 0.21 s, cold first call 0.75 s (excluded below).

| set | clips | audio s | WER % | CER % | CER no-accent % | RTF | p50 s | p95 s |
|---|---|---|---|---|---|---|---|---|
| prompts/hu | 5 | 18.6 | 5.3 | 0.8 | 0.8 | 0.250 | 0.98 | 1.00 |
| prompts/en | 3 | 11.7 | 0.0 | 0.0 | 0.0 | 0.235 | 0.92 | 0.92 |
| all | 8 | 30.3 | 3.0 | 0.5 | 0.5 | 0.244 | 0.92 | 1.00 |

| id | lang | audio s | latency s | WER % | reference | output |
|---|---|---|---|---|---|---|
| hu1 | hu | 3.5 | 0.78 | 0.0 | Holnap délután háromkor találkozunk az irodában. | Holnap délután háromkor találkozunk az irodában. |
| hu2 | hu | 3.4 | 0.88 | 0.0 | Kérlek, küldd el a jelentést péntekig a csapatnak. | Kérlek, küldd el a jelentést péntekig a csapatnak. |
| hu3 | hu | 4.0 | 1.00 | 0.0 | A szoftverfejlesztés legnehezebb része a nevek kitalálása. | A szoftverfejlesztés legnehezebb része a nevek kitalálása. |
| hu4 | hu | 3.7 | 1.00 | 20.0 | Tegnap pusholtam a branchet, de a build még mindig piros. | Tegnap pusholtam a Branhet, de a bujild még mindig piros. |
| hu5 | hu | 4.2 | 0.98 | 0.0 | Összesen huszonhét ügyfél jelentkezett az új szolgáltatásra. | Összesen 27 ügyfél jelentkezett az új szolgáltatásra. |
| en1 | en | 4.0 | 0.92 | 0.0 | Let's schedule the product review for next Tuesday morning. | Let's schedule the product review for next Tuesday morning. |
| en2 | en | 4.1 | 0.92 | 0.0 | The latency dropped by forty percent after the last release. | The latency dropped by 40% after the last release. |
| en3 | en | 3.7 | 0.92 | 0.0 | Please remind me to call the accountant before lunch. | Please remind me to call the accountant before lunch. |

## Notes

- The only errors are the mixed HU-EN tech words in hu4 ("branchet" → "Branhet", "build" → "bujild"), as in e2e-01. D1 target.
- Warm latency here is 0.8–1.0 s for 3.4–4.2 s clips (e2e-01 measured ~0.72 s through the MCP server; this run shared the machine with an app build, so treat the absolute numbers loosely). RTF ≈ 0.24: the 30 s encoder window dominates (B3).

## Reproduce

```bash
(cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper)
core/target/release/pusmogo-bench --json /tmp/b.json --md /tmp/b.md bench/audio/e2e/prompts.tsv   # clips: see e2e-01.md
```
