---
name: screenshot-ui
description: Render Pusmogo's sound bar or onboarding window and capture a screenshot to verify a UI change visually. Use after any change under app/ that affects what the user sees.
---

# Check the UI with a screenshot

Up: [../../../AGENTS.md](../../../AGENTS.md)
Verified: 2026-11-09

A UI change is verified when you have looked at a screenshot, not when the SwiftUI code reads right.

1. **Build:** `./scripts/build.sh` (stub engine is enough for UI) or `./scripts/build.sh --whisper`.
2. **Another copy running?** `pgrep -x Pusmogo`. An ordinary launch (including `PUSMOGO_DEMO` and onboarding) asks an older running copy to open its panel and quits, so nothing of yours is on screen. Then use the one-shot routes, which run next to it without a key tap: `PUSMOGO_PREVIEW_HOVER=1` for the idle bar, `PUSMOGO_SELFTEST_WAV=<clip>` with `PUSMOGO_ONBOARDING_STEP=<0-6>` for one onboarding step (see [app/AGENTS.md](../../../app/AGENTS.md)). Never quit the other copy to make room.
3. **Sound bar.** With no other copy running, `PUSMOGO_DEMO=1` cycles the bar through every state. Always give test launches a scratch profile and keep the dictation record off, so they never touch the real History or recordings:
   ```bash
   PUSMOGO_PROFILE=/tmp/pusmogo-shot.sqlite PUSMOGO_KEEP_AUDIO=0 PUSMOGO_DEMO=1 \
     ./build/Pusmogo.app/Contents/MacOS/Pusmogo &
   sleep 8
   ```
   Capture the bar by its window id, not a screen region: list the windows owned by `Pusmogo` with `CGWindowListCopyWindowInfo` (the bar window is 360×112 points, bottom centre), then `screencapture -o -x -l<id> /tmp/bar.png`. Another dictation app may draw its own bar in the same spot; the window id keeps it out.
4. **Onboarding.** Show the first-run window with a launch argument instead of deleting the saved flag (`defaults delete` would also reset any installed copy, which shares the defaults domain), then let it step through on a timer:
   ```bash
   PUSMOGO_PROFILE=/tmp/pusmogo-shot.sqlite PUSMOGO_KEEP_AUDIO=0 PUSMOGO_ONBOARDING_CYCLE=2.5 \
     ./build/Pusmogo.app/Contents/MacOS/Pusmogo -onboarded NO &
   ```
   The window (720 points wide) is centred; capture it by window id once per step.
5. **Look** at every PNG (open it, or read it as an image if you are an agent). Check against `app/AGENTS.md`: design tokens only, Hungarian copy without exclamation marks or em dashes.
6. **Clean up:** `pkill -f "build/Pusmogo.app/Contents/MacOS/Pusmogo"` (only the copy you started, never an installed one) and `rm -f /tmp/pusmogo-shot.sqlite`. Commit only small, useful screenshots, under `docs/screens/`.
