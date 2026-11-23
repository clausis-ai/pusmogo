# Pusmogo: guide for coding agents

Up: [README.md](README.md) · Docs index: [docs/README.md](docs/README.md)
Verified: 2026-11-08

Pusmogo is a small native macOS menu bar app for voice dictation: hold a key, speak Hungarian or English, release, and the text is typed into the focused app. Everything runs locally. A Swift app (`app/`) sits on a Rust core (`core/`) that holds the speech engine, the text cleanup and a C ABI.

This file is for agents (Claude Code, Codex and others). `CLAUDE.md` is a symlink to it. Humans should start with [CONTRIBUTING.md](CONTRIBUTING.md); everything here agrees with it.

## Hard rules

- **Licence is Apache-2.0.** Never paste code from GPL projects. Never use code, assets or details obtained by inspecting another app's bundle, binaries or local data.
- **The repository is public.** Commits, comments and docs are read by strangers. Use Conventional Commits (`feat(core): …`, `fix(app): …`, `docs: …`), an imperative subject of at most 72 characters, and a body that says why. Never write personal names or emails, home-directory paths, private repositories, secrets, customer data or business strategy. Compare with other products only through public information.
- **Never commit** model weights, recordings of real people, or any file over 1 MB. Models live in `~/Library/Application Support/Pusmogo/models/`.
- **Never** change the user's macOS settings (keyboard, permissions, login items), quit or modify other apps, push to `main`, force-push, or add `.github/workflows/` files.
- **Scope of v1:** Hungarian and English only, chosen by the user (fn + ⇧). No automatic language detection.
- **Logic goes in Rust** where it can be unit-tested; Swift is only for macOS APIs.

## Build and test

```bash
cd core && cargo test --all                          # unit tests (stub engine, no model)
cd app && swift test                                 # hotkey state machine tests (needs Xcode)
cd core && cargo clippy --all-targets -- -D warnings # must be clean
./scripts/build.sh            # app with the stub engine, fast
./scripts/build.sh --whisper  # app with whisper.cpp + Metal (needs cmake)
./scripts/build.sh --universal # same, arm64 + x86_64 (needs the x86_64-apple-darwin Rust target)
du -sk build/Pusmogo.app      # must stay under 10240 KB
./scripts/make-dmg.sh         # build/Pusmogo.dmg, drag-to-Applications (needs create-dmg)
./scripts/check-agent-docs.sh # agent docs: links and CLAUDE.md siblings
```

Speech accuracy and latency: the `bench` skill in [.agents/skills/bench/SKILL.md](.agents/skills/bench/SKILL.md). UI changes: the `screenshot-ui` skill in [.agents/skills/screenshot-ui/SKILL.md](.agents/skills/screenshot-ui/SKILL.md). A change is verified when you have run it, not when the code reads right.

## If you touch X, read Y

| You touch | Read first |
|---|---|
| `app/` (Swift, UI, hotkeys, paste, audio) | [app/AGENTS.md](app/AGENTS.md) |
| `core/` (Rust engine, cleanup, C ABI, MCP, bench) | [core/AGENTS.md](core/AGENTS.md) |
| `core/crates/pusmogo-core/src/ffi.rs` | [core/include/pusmogo.h](core/include/pusmogo.h): keep both in sync in one commit |
| Speech quality or speed | [docs/bench/](docs/README.md) and the `bench` skill |
| Build, bundle, signing | [scripts/build.sh](scripts/build.sh) |
| These agent docs or skills | run [scripts/check-agent-docs.sh](scripts/check-agent-docs.sh) |
