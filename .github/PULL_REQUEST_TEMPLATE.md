## What and why

<!-- What does this change do, and why is it needed? Link the issue: "Closes #123". -->

## How it was tested

<!-- Commands you ran and their results. For UI changes, add a screenshot. For speech changes, add benchmark numbers before and after. -->

## Checklist

- [ ] `cd core && cargo test --all` passes
- [ ] `cd core && cargo clippy --all-targets -- -D warnings` passes
- [ ] `./scripts/build.sh --whisper` builds and `build/Pusmogo.app` stays under 10 MB
- [ ] C ABI changes are mirrored in `core/include/pusmogo.h`
- [ ] Commit messages follow [Conventional Commits](https://www.conventionalcommits.org)
- [ ] No code copied from GPL projects, no model weights or real voice recordings committed
