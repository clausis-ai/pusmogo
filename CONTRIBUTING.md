# Contributing to Pusmogo

Thanks for helping. Pusmogo is a small native macOS dictation app: a Swift menu bar app (`app/`) on top of a Rust core (`core/`). This page covers how to build it, test it and send changes.

By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md). Security issues go through [SECURITY.md](SECURITY.md), not public issues.

## Before you start

- For anything larger than a bug fix, open an issue first so we can agree on the approach.
- v1 is Hungarian and English only, chosen by the user (fn + ⇧). Automatic language detection is out of scope for v1.

## Requirements

- macOS 14 or newer, Apple Silicon recommended
- Xcode command line tools (Swift 5.10+)
- Rust via [rustup](https://rustup.rs)
- `cmake` (`brew install cmake`) for the real Whisper engine

## Build and run

```bash
./scripts/build.sh            # stub engine, fast, no cmake needed
./scripts/build.sh --whisper  # real engine: whisper.cpp + Metal
./scripts/build.sh --universal # real engine for Apple Silicon and Intel (rustup target add x86_64-apple-darwin)
open build/Pusmogo.app
```

The speech model is downloaded separately (see [README.md](README.md)) into `~/Library/Application Support/Pusmogo/models/`. Never commit model weights.

The app bundle must stay under 10 MB: check with `du -sk build/Pusmogo.app`.

## Tests

```bash
cd core && cargo test --all
(cd app && swift test)   # hotkey state machine; needs Xcode
cd core && cargo clippy --all-targets -- -D warnings
```

All three must pass before a pull request is merged. Logic belongs in the Rust core, where it can be tested without a GUI; Swift handles only the macOS-specific parts.

If you change the C ABI in `core/crates/pusmogo-core/src/ffi.rs`, update `core/include/pusmogo.h` in the same commit.

For UI changes, attach a screenshot to the pull request. `PUSMOGO_DEMO=1` cycles the sound bar through its states; `PUSMOGO_ONBOARDING_CYCLE=2.5` steps through onboarding (launch with `-onboarded NO` after the executable path; see the `screenshot-ui` skill for running next to an installed copy).

For speech changes, run the benchmark (`core/crates/pusmogo-bench`) before and after and include the numbers.

## Commits

We use [Conventional Commits](https://www.conventionalcommits.org):

```
feat(core): add spoken punctuation for Hungarian
fix(app): keep the sound bar above full-screen apps
docs: explain the model download
```

- Imperative subject line, at most 72 characters.
- The body explains *why* the change is needed.
- Common types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `chore`. Common scopes: `core`, `app`, `mcp`, `bench`, `scripts`.

Signing off your commits (`git commit -s`, [Developer Certificate of Origin](https://developercertificate.org)) is welcome but optional.

## Licence and third-party code

Pusmogo is licensed under [Apache-2.0](LICENSE). By contributing you agree that your contribution is licensed under the same terms.

- Do not copy code from GPL-licensed projects.
- Do not include code, assets or details obtained by inspecting other applications' binaries or bundles.
- The "Pusmogo" and "vatson.ai" names are not covered by the licence (see [NOTICE](NOTICE)); forks need a different name.

## Writing style

- UI copy is Hungarian, informal (*te*), with no exclamation marks. English strings come through localisation.
- Code comments, commits and docs are in English.
