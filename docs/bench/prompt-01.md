# prompt-01: English words in the prompt and short Hungarian dictations

Up: [../README.md](../README.md)
Verified: 2026-11-08

A first test with a real voice turned short Hungarian phrases into English- or Spanish-looking spellings ("lopás" → "López", "haló" → "Való"). This report checks whether the engine prompt can cause that. With an empty dictionary and no context hints the app sends **no prompt at all**, so the question is what happens once a dictionary or the focused field adds words.

## Result

| prompt | short hu | short low | short noisy | short, all | exact (of 132) | empty | FLEURS hu |
|---|---|---|---|---|---|---|---|
| none (default) | 31.5 | 23.9 | 50.0 | 35.1 | 69 | 4 | 17.3 |
| context: a terminal (git, cargo, a macOS error) | 39.1 | 30.4 | 54.3 | 41.3 | 58 | 4 | 17.0 |
| context: one line of Spanish names | 35.9 | 29.3 | 52.2 | 39.1 | 61 | 7 | 17.4 |
| context: an English team chat message | 39.1 | 28.3 | 54.3 | 40.6 | 58 | 5 | 17.6 |
| dictionary: 15 English product names | 38.0 | 28.3 | 55.4 | 40.6 | 58 | 6 | 17.3 |

WER %, large-v3-turbo q5_0, beam search as in [beam-01](beam-01.md), Apple M5 Pro, `pusmogo-bench`, Silero speech detector on. Synthetic speech (macOS TTS) is easier than a real voice. Hints were extracted with `context::hint_words` exactly as the app does: the terminal gave "Sun Nov, ttys003, System Settings, Privacy, Security, Microphone"; the chat gave "Monday, Michael, Slack, Google Drive, Notion, Figma, Settings, Stripe, Friday, Jessica". Prompts were 20–50 tokens, all labelled "Gyakori szavak:".

## Findings

1. **Any English word list in the prompt makes short Hungarian dictations worse**: 35.1 → 39.1–41.3 % WER over the 132 short clips, 8–11 fewer exact clips. The errors are the same kind as in the real-voice test: Hungarian sounds respelt the English way. "Helló" → "Hello", "Szia" → "See ya", "Hétfő" → "Heetsző", "Hétfőn ráérsz?" → "Heet ura eers.", "Lámpa" → "Lampa", "Rizs" → "Riz", "Rendben" → "Rentben". In noise it gets worse ("Hívj fel később." → "He's a fack issue.").
2. **Full sentences do not care.** FLEURS stays within 17.0–17.6 % under every prompt. With a few seconds of Hungarian audio the decoder follows the audio; with under a second it follows the prompt's language.
3. **The prompt alone does not explain "López".** Spanish surnames in the prompt produced no Spanish-looking output on any clip, but did add empty outputs (4 → 7). The default prompt is empty, and the dictionary of the test profile was empty, so the real-voice "López" came from the engine (or from context hints of the focused app, which are not recorded).
4. **Context hints are on by default and work silently**, so a user dictating Hungarian in a terminal, a code editor or an English chat gets the worse numbers above with no visible cause.

## What this means

The prompt should not bias short Hungarian dictations. Options for the fix, to be measured on the same five conditions plus the name-hint sets of [context-01](context-01.md):

- leave context hints out of the prompt for very short clips and rely on respelling (`context::respell`) for names; note that the name-hint test clips are 2.6–3.3 s long while the short set is 0.6–2.6 s, so a plain length cutoff needs care;
- put a short Hungarian carrier sentence before the word list so the prompt reads as Hungarian text;
- leave out hints that are plain English words (Monday, Settings, Security) when dictating Hungarian.

## Reproduce

```bash
cd core && cargo build -j 4 --release -p pusmogo-bench --features whisper
S=bench/audio/short F=bench/audio/fleurs-hu/hu.tsv
core/target/release/pusmogo-bench --json none.json $S/hu.tsv $S/low.tsv $S/noisy.tsv $F
core/target/release/pusmogo-bench --json term.json --context-file term.txt $S/hu.tsv $S/low.tsv $S/noisy.tsv $F
core/target/release/pusmogo-bench --json dict.json --dict-file dict-en.txt $S/hu.tsv $S/low.tsv $S/noisy.tsv $F
```

The context and dictionary files used are listed below.

- `term.txt`: a terminal session with `git status`, `cargo test --all` output and the line "Error: Permission denied. Please check System Settings > Privacy & Security > Microphone."
- `spanish.txt`: "Warning: Lopez Sanchez Rodriguez García Martinez merged pull request #42 from Madrid"
- `chat.txt`: "Hey team, quick update from the Monday standup. Sarah and Michael reviewed the Slack integration with Google Drive and Notion. The Figma designs for the Settings page are ready. Next: ship the Stripe checkout on Friday. Thanks, Jessica."
- `dict-en.txt`: GitHub, Slack, Notion, Figma, Claude, OpenAI, Kubernetes, TypeScript, JavaScript, PostgreSQL, Vercel, Cloudflare, Stripe, Linear, Zoom (one per line)
