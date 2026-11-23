#!/usr/bin/env bash
# Builds build/Pusmogo.dmg: the app next to an Applications link on a plain background,
# so a user installs by dragging one onto the other.
#   ./scripts/make-dmg.sh              build the app (--whisper), then the disk image
#   ./scripts/make-dmg.sh --no-build   package the existing build/Pusmogo.app
# Needs create-dmg (MIT): brew install create-dmg. It lays out the window by asking
# Finder through AppleScript, so the first run may show an Automation prompt.
# The background comes from app/dmg/background.swift; PUSMOGO_DMG_BACKGROUND=<png|tiff>
# uses another image (640 × 428 points) instead. The image is not signed or notarised.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/build/Pusmogo.app"
OUT="$ROOT/build/Pusmogo.dmg"

case "$#:${1:-}" in
  0: | 1:--no-build) ;;
  1:-h | 1:--help) echo "usage: ./scripts/make-dmg.sh [--no-build]"; exit 0 ;;
  *) echo "usage: ./scripts/make-dmg.sh [--no-build]" >&2; exit 2 ;;
esac
command -v create-dmg >/dev/null || { echo "create-dmg not found: brew install create-dmg" >&2; exit 1; }
[[ "${1:-}" == "--no-build" ]] || "$ROOT/scripts/build.sh" --whisper
[[ -d "$APP" ]] || { echo "no $APP: run ./scripts/build.sh --whisper first" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "→ background"
if [[ -n "${PUSMOGO_DMG_BACKGROUND:-}" ]]; then
  BG="$PUSMOGO_DMG_BACKGROUND"
else
  swift "$ROOT/app/dmg/background.swift" "$WORK"
  # One TIFF holding both sizes, so Finder picks the sharp one on Retina screens.
  tiffutil -cathidpicheck "$WORK/background.png" "$WORK/background@2x.png" -out "$WORK/background.tiff" >/dev/null 2>&1
  BG="$WORK/background.tiff"
fi

echo "→ stage"
mkdir "$WORK/stage"
# ditto keeps the signature, extended attributes and symlinks intact.
ditto "$APP" "$WORK/stage/Pusmogo.app"

echo "→ disk image"
rm -f "$OUT"
# Icon positions are icon centres in the 640 × 428 window; background.swift draws the
# arrow and hint around them.
create-dmg \
  --volname Pusmogo \
  --volicon "$ROOT/app/AppIcon.icns" \
  --background "$BG" \
  --window-size 640 428 \
  --icon-size 128 \
  --text-size 13 \
  --icon Pusmogo.app 160 200 \
  --hide-extension Pusmogo.app \
  --app-drop-link 480 200 \
  --no-internet-enable \
  "$OUT" "$WORK/stage" >"$WORK/create-dmg.log" 2>&1 || { cat "$WORK/create-dmg.log" >&2; exit 1; }

hdiutil verify "$OUT" >/dev/null
echo "✓ $OUT ($(du -h "$OUT" | cut -f1))"
