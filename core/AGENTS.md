# core/: the Rust workspace

Up: [../AGENTS.md](../AGENTS.md)
Verified: 2026-11-09

## Crates

| Crate | Role |
|---|---|
| `crates/pusmogo-core` | Engine trait, Whisper engine, cleanup, audio helpers, C ABI. Built as a static library for the app. |
| `crates/pusmogo-mcp` | stdio MCP server over the core: `transcribe_file`, `clean_text`, and read-only profile tools `search_history`, `get_dictionary`, `list_snippets` (`Profile::open_read_only`; each refuses unless the profile setting `mcp.shareHistory` / `mcp.shareDictionary` is "1", off by default). `../scripts/build.sh` bundles a build without `whisper` as `Contents/MacOS/pusmogo-mcp` (own target dir `target/mcp`); that build leaves `transcribe_file` out of its tool list. Notes are not shared: there is no notes feature yet, and a future notes tool needs its own share setting, off by default |
| `crates/pusmogo-bench` | Benchmark CLI: WER, CER, accent-folded CER, real-time factor, p50/p95 latency |

Features of `pusmogo-core`: `whisper` (whisper.cpp with Metal, needs cmake) and `wav` (WAV reading for the MCP server and the bench, not linked into the app). Without `whisper`, `engine::load` returns the `StubEngine`, so tests and the app run without a model.

## Key parts

- `src/engine.rs`: the `Engine` trait (`transcribe(samples, lang, prompt)`), `Language`, `StubEngine`, `load()`. A new engine implements the trait and is chosen in `load()`; the app does not change.
- `src/cleanup.rs`: text cleanup after recognition and the personal `Dictionary` (replacements plus a prompt for the engine). Pure functions, test them directly.
- `src/lang.rs` + `src/lang/{hu,en}.toml`: per-language word lists the cleanup reads (fillers, stutter words, abbreviations); see [../docs/adding-a-language.md](../docs/adding-a-language.md).
- `src/backtrack.rs`: spoken self-corrections ("háromkor, nem, négykor" → "négykor"), a word-dropping pass after `clean()`. On in the app by default (Settings "Catch self-corrections", C ABI `pusmogo_set_self_corrections`); when it drops words, the text before it is kept for History (`pusmogo_last_spoken`, stored with `pusmogo_profile_set_spoken` in `history.spoken`, profile migration 5; `history_json` adds `spoken` only when set); measured in `docs/bench/polish-03.md` (`clean_text` `"backtrack": true`).
- `src/style.rs`: writing styles per app, a pass after `clean()`. `category_for(bundle id, window title, user mapping)` gives personal / work / email / code / other (browsers by a whole title part such as "Gmail"); `apply` makes Formal (full stops), Casual (no stop on a one-sentence message) or Very casual (lower-case start); email also puts a spoken greeting and sign-off on their own lines and never invents one. C ABI `pusmogo_style_category` / `pusmogo_style_apply`; MCP `clean_text` `app` + `style`.
- `src/lists.rs`: lists by voice, run by `style::apply` (and inside an email body). Two or more clearly marked items ("először …, másodszor …", "egy, tej, kettő, kenyér", "first …, second …", "number one …", "1. … 2. …", "bullet point …", "listapont …") become "1." or "•" lines, "-" in code, one plain line per item in chat apps. Ordinals and numbers must run 1, 2, 3 without a gap and every item must hold a word, so a lone "Először …" or a count stays prose.
- `src/code.rs`: the code style for editors, IDEs and terminals (`style::Category::Code`): no capital at the start, no final full stop, straight quotes, spoken case commands ("camel case user id" → `userId`, also snake, kebab, pascal, constant; glued engine output such as "CamelCaseUserID" too), and spoken file tags for coding chats ("tag index dot tsx", "címke index pont tsx" → `@index.tsx`; only with a known extension).
- `src/context.rs`: context hints, name-like words from the text around the cursor that go into the engine prompt after the dictionary (`pusmogo_set_context`; `pusmogo_set_code_context` for editors and terminals also takes snake/constant-case identifiers and file names with a known extension, from the field and the window title). Measured in `docs/bench/context-01.md`.
- `src/edits.rs`: learning from edits. `learned_correction` compares the field right after an insertion with a later read and returns a misheard word the user fixed (up to three words, inside the dictated text), for an "Add to dictionary?" offer. `Profile::note_correction` records every such fix (`corrections.accepted` 0, kept out of the most-corrected insight) and adds the word to the dictionary without asking the second time the same fix is seen (C ABI `pusmogo_profile_note_correction`, 1 = learned).
- `src/vad.rs`: the hallucination guard: a frame-energy speech check before inference and a filter for phrases Whisper invents on silence. Tune it with the `silence` bench set.
- `src/spell.rs`: spelling repair for Hungarian after `clean()`: a lower-case word unknown to the dictionary becomes the one known word that sounds the same; names, known words and the user's words stay. Dictionary in `data/` (MPL-2.0, `scripts/make-spell-dict.py`), freed with the engine; numbers in `docs/bench/polish-09.md`.
- `src/session.rs`: streaming dictation. `Session` takes audio while the user speaks, `step()` transcribes finished chunks cut in pauses, `finish()` makes one final pass of up to 29 s. `transcribe_all()` is the one-shot path for a whole recording: over 30 s it cuts at pauses the same way (`docs/bench/longprompt-01.md`). Tests use a recording fake engine; measure changes with `pusmogo-bench --stream` (`docs/bench/stream-01.md`).
- `src/models.rs`: which speech model a language loads (`wanted`, `pick`): large-v3 for Hungarian above 8 GB of memory, turbo for English and smaller Macs, an explicit Settings choice first, falling back to whichever model is installed.
- `src/snippets.rs`: snippets, a spoken trigger that inserts a saved text verbatim. `match_key` folds case, accents and punctuation; only a whole utterance fires a trigger. Stored in the profile (`set_snippet`, `snippet_for`), C ABI `pusmogo_profile_*snippet*`.
- `src/testset.rs`: the test-set recording mode's logic. The reading script `../bench/testset/script.tsv` is compiled in; `plan` lists the takes (every line normally, then the `whisper=yes` lines whispered), `save_take` writes `<id>.wav` / `<id>-whisper.wav` (16 kHz mono) plus the reference `.txt` in a speaker folder (`speaker_dir`), `write_consent` the `consent.txt` that `../scripts/testset-manifest.py` reads. C ABI `pusmogo_testset_*`; the app's recording window is `../app/Sources/Pusmogo/TestSetWindow.swift` (Settings → Test recording, or `PUSMOGO_TESTSET=<speaker>`).
- `src/insights.rs`: usage insights from the profile (`Profile::insights`): words, words per minute (only rows with a stored recording length, `set_history_audio_ms`), current and longest daily streak in local days, most used word (Hungarian and English stop words skipped), most corrected word, peak hour and app, and a rule-picked title key the app words. Pure function `compute`, JSON via `to_json`.
- `src/mcp_config.rs`: registering `pusmogo-mcp` with local AI apps (Claude Desktop, Claude Code user scope, Cursor). Adds or removes only the `mcpServers.pusmogo` entry, keeping every other key, its order and its numbers; `preview_install` returns a line diff to show first; a write backs up the old file to `<file>.pusmogo-backup` and replaces it atomically; a file that is not valid JSON is never touched. Undo removes our entry rather than restoring the backup, because the clients rewrite these files themselves. The app calls it from Settings → AI apps (`McpClientsView.swift`).
- `src/profile.rs`: the local profile, one SQLite file (macOS system `libsqlite3`, not bundled) with history, dictionary, corrections, settings and snippets. Schema migrations by `PRAGMA user_version`: append to `MIGRATIONS`, never edit a shipped one. Export/import copy the whole file. The app opens it with `open_or_recover` (C ABI `pusmogo_profile_open_recovering`): a file that is not SQLite or fails `quick_check` is moved aside as `<name>.broken-<secs>.<ext>` and a fresh one started; a profile from a newer version is refused and never written to (`pusmogo_profile_is_newer` tells the app why, read-only, so Home and History can ask the user to update).
- `src/debug.rs`: the opt-in local record of the last `KEEP` (20) dictations for replaying failures: the 16 kHz audio the engine got plus a JSON of raw and cleaned text, language, prompt and hint words. Off by default; the app sets the folder with `pusmogo_set_debug_dir` (tests force it off with `PUSMOGO_KEEP_AUDIO=0` or redirect it with `PUSMOGO_DEBUG_DIR`). `write_wav` is shared with `testset.rs`.
- `src/audio.rs`: WAV reading and resampling to 16 kHz mono.
- `src/ffi.rs`: the C ABI. **Every change here must be mirrored in [include/pusmogo.h](include/pusmogo.h) in the same commit**; the Swift wrapper is `../app/Sources/Pusmogo/Core.swift`. Strings returned to C are freed with `pusmogo_string_free`.

## Tests and gates

```bash
cargo test --all
cargo clippy --all-targets -- -D warnings
cargo build -j 4 --release -p pusmogo-mcp --features whisper   # real engine, a few minutes
```

Unit tests use the stub engine and need no model. Add a test with every behaviour change in cleanup, the normaliser or the metrics. Speech changes need bench numbers before and after: see the `bench` skill ([../.agents/skills/bench/SKILL.md](../.agents/skills/bench/SKILL.md)). Hungarian test audio can be synthesised with macOS TTS; say so when reporting, because synthetic speech is easier than real speech.

Adding, removing or upgrading a dependency changes what the app bundle links: run `../scripts/make-notices.py` and commit the regenerated `THIRD_PARTY_NOTICES.md` (`--check` tells you whether it is stale). `build.sh` copies it with `LICENSE` and `NOTICE` into the app.
