#!/usr/bin/env bash
# Builds build/Pusmogo.app: Rust core (static lib) → Swift app → bundle → ad-hoc sign.
#   ./scripts/build.sh            stub engine (no cmake, fast)
#   ./scripts/build.sh --whisper  real engine: whisper.cpp + Metal
#   ./scripts/build.sh --universal  real engine, one app for Apple Silicon and Intel
set -euo pipefail
export MACOSX_DEPLOYMENT_TARGET=13.0
# Prefer rustup (its std targets old macOS); Homebrew rust is built for the host OS only.
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FEATURES=()
UNIVERSAL=0
usage() { sed -n '3,5s/^#//p' "$0" >&2; exit "${1:-2}"; }
# A misspelt flag must not quietly build the stub engine, an app that types no speech.
[[ $# -le 1 ]] || usage
case "${1:-}" in
  "") ;;
  -h|--help) usage 0 ;;
  --whisper) FEATURES=(--features pusmogo-core/whisper) ;;
  --universal) FEATURES=(--features pusmogo-core/whisper); UNIVERSAL=1 ;;
  *) echo "unknown option: $1" >&2; usage ;;
esac

echo "→ rust core"
(cd "$ROOT/core" && cargo build --release -p pusmogo-core ${FEATURES[@]+"${FEATURES[@]}"})

# The app links whatever library is staged in core/target/lib (see app/Package.swift):
# the host build, or a fat arm64 + x86_64 one for --universal.
LIB="$ROOT/core/target/lib"
mkdir -p "$LIB"
SWIFT_ARGS=()
if [[ $UNIVERSAL == 1 ]]; then
  echo "→ rust core (x86_64)"
  # whisper.cpp switches off every x86 vector extension when cross-compiling.
  # Every Intel Mac that runs macOS 13 or later has AVX2, FMA and F16C.
  X86_FLAGS="-mavx -mavx2 -mfma -mf16c -mbmi2 -msse4.2"
  (cd "$ROOT/core" && CFLAGS_x86_64_apple_darwin="$X86_FLAGS" CXXFLAGS_x86_64_apple_darwin="$X86_FLAGS" \
    cargo build --release -p pusmogo-core --target x86_64-apple-darwin ${FEATURES[@]+"${FEATURES[@]}"})
  # whisper-rs-sys does not rebuild when these flags change: refuse a cached build without them.
  if ! grep -q -- "-mavx2" "$ROOT"/core/target/x86_64-apple-darwin/release/build/whisper-rs-sys-*/out/build/CMakeCache.txt; then
    echo "x86_64 whisper.cpp was built without AVX2; run: (cd core && cargo clean --release --target x86_64-apple-darwin -p whisper-rs-sys)" >&2
    exit 1
  fi
  lipo -create "$ROOT/core/target/release/libpusmogo_core.a" \
    "$ROOT/core/target/x86_64-apple-darwin/release/libpusmogo_core.a" -output "$LIB/libpusmogo_core.a"
  SWIFT_ARGS=(--arch arm64 --arch x86_64)
else
  cp "$ROOT/core/target/release/libpusmogo_core.a" "$LIB/libpusmogo_core.a"
fi
# Cargo and whisper-rs-sys keep objects built for an older deployment target in their
# caches; such a library links with only a warning but needs a newer macOS to run.
NEWEST=$(otool -l "$LIB/libpusmogo_core.a" | awk '$1=="minos"{print $2}' | sort -V | tail -1)
if [[ -n "$NEWEST" && "$(printf '%s\n%s\n' "$NEWEST" "$MACOSX_DEPLOYMENT_TARGET" | sort -V | tail -1)" != "$MACOSX_DEPLOYMENT_TARGET" ]]; then
  echo "core objects target macOS $NEWEST, above $MACOSX_DEPLOYMENT_TARGET; run: (cd core && cargo clean --release; cargo clean --release --target x86_64-apple-darwin)" >&2
  exit 1
fi

# The MCP server for local AI apps, bundled next to the app: built without the
# speech engine (text cleanup and the read-only profile tools, ~0.7 MB) so the
# app stays under its 10 MB budget. Its own target dir keeps the staged core
# library above, and the cached whisper build, untouched.
echo "→ mcp server"
MCP_TARGET="$ROOT/core/target/mcp"
(cd "$ROOT/core" && CARGO_TARGET_DIR="$MCP_TARGET" cargo build --release -p pusmogo-mcp)
MCP_BIN="$MCP_TARGET/release/pusmogo-mcp"
if [[ $UNIVERSAL == 1 ]]; then
  (cd "$ROOT/core" && CARGO_TARGET_DIR="$MCP_TARGET" cargo build --release -p pusmogo-mcp --target x86_64-apple-darwin)
  lipo -create "$MCP_BIN" "$MCP_TARGET/x86_64-apple-darwin/release/pusmogo-mcp" -output "$MCP_TARGET/pusmogo-mcp-universal"
  MCP_BIN="$MCP_TARGET/pusmogo-mcp-universal"
fi

echo "→ swift app"
# SwiftPM does not track the Rust static library, so a core-only change would
# leave the old core linked in. Removing the binary forces a fresh link.
rm -f "$ROOT/app/.build/release/Pusmogo" "$ROOT/app/.build/apple/Products/Release/Pusmogo"
(cd "$ROOT/app" && swift build -c release ${SWIFT_ARGS[@]+"${SWIFT_ARGS[@]}"})
BIN="$ROOT/app/.build/release/Pusmogo"
[[ $UNIVERSAL == 1 ]] && BIN="$ROOT/app/.build/apple/Products/Release/Pusmogo"

echo "→ bundle"
APP="$ROOT/build/Pusmogo.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/Pusmogo"
cp "$MCP_BIN" "$APP/Contents/MacOS/pusmogo-mcp"
strip -x "$APP/Contents/MacOS/pusmogo-mcp"
cp "$ROOT/app/Info.plist" "$APP/Contents/Info.plist"
# Regenerate with: swift scripts/make-icon.swift
cp "$ROOT/app/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
# Licence texts travel with every copy (whisper.cpp/ggml and the crates are MIT
# and similar). Regenerate the third-party file with: scripts/make-notices.py
cp "$ROOT/LICENSE" "$ROOT/NOTICE" "$ROOT/THIRD_PARTY_NOTICES.md" "$APP/Contents/Resources/"
# Localised strings: Hungarian is the source text, en.lproj holds English.
cp -R "$ROOT/app/Resources/"*.lproj "$APP/Contents/Resources/"
for f in "$APP"/Contents/Resources/*.lproj/*.strings; do plutil -lint -s "$f"; done
# Dropping local symbols keeps the app under 10 MB.
strip -x "$APP/Contents/MacOS/Pusmogo"
# macOS 26 icon (light, dark, tinted, clear) from the Icon Composer document; older
# macOS keeps AppIcon.icns. The compiled catalog is ~1.8 MB of pre-rendered images,
# so it is kept only while the app stays under the 10 MB budget (not --universal).
ICON_TMP="$(mktemp -d)"
if xcrun --find actool >/dev/null 2>&1 &&
   xcrun actool --compile "$ICON_TMP" --platform macosx --minimum-deployment-target 13.0 \
     --app-icon AppIcon --output-partial-info-plist "$ICON_TMP/icon.plist" \
     "$ROOT/app/AppIcon.icon" >/dev/null 2>&1 &&
   (( $(du -sk "$APP" | cut -f1) + $(du -sk "$ICON_TMP/Assets.car" | cut -f1) < 10240 )); then
  cp "$ICON_TMP/Assets.car" "$APP/Contents/Resources/Assets.car"
  plutil -replace CFBundleIconName -string AppIcon "$APP/Contents/Info.plist"
else
  echo "  (no macOS 26 icon: actool missing or the app would pass 10 MB; AppIcon.icns only)"
fi
rm -rf "$ICON_TMP"
# Sparkle, without what this app does not use: headers, the sandbox helpers, other
# languages and, in a one-architecture build, the other architecture.
echo "→ updater"
SPARKLE="$(find "$ROOT/app/.build/artifacts" -type d -path '*macos*' -name Sparkle.framework | head -1)"
[[ -n "$SPARKLE" ]] || { echo "Sparkle.framework not found under app/.build/artifacts" >&2; exit 1; }
FW="$APP/Contents/Frameworks/Sparkle.framework"
mkdir -p "$APP/Contents/Frameworks"
ditto "$SPARKLE" "$FW"
rm -rf "$FW"/{Headers,PrivateHeaders,Modules,XPCServices} "$FW"/Versions/B/{Headers,PrivateHeaders,Modules,XPCServices}
find "$FW/Versions/B" -name '*.lproj' ! -name hu.lproj ! -name en.lproj ! -name Base.lproj -prune -exec rm -rf {} +
if [[ $UNIVERSAL == 0 ]]; then
  for f in "$FW/Versions/B/Sparkle" "$FW/Versions/B/Autoupdate" "$FW/Versions/B/Updater.app/Contents/MacOS/Updater"; do
    lipo "$f" -thin "$(uname -m)" -output "$f.thin" && mv "$f.thin" "$f"
  done
fi

# Stable signature keeps macOS permissions across rebuilds. Set PUSMOGO_SIGN_ID to an
# "Apple Development: ..." identity (free via Xcode > Settings > Accounts). Ad-hoc otherwise:
# then macOS asks for Accessibility again after every rebuild. A real identity signs with
# the hardened runtime; a "Developer ID" one also with Apple's timestamp (notarisation
# needs both). Inner code first: each seal covers what is inside it.
SIGN=(--force --sign "${PUSMOGO_SIGN_ID:--}")
[[ "${PUSMOGO_SIGN_ID:--}" == "-" ]] || SIGN+=(--options runtime)
[[ "${PUSMOGO_SIGN_ID:-}" == "Developer ID"* ]] && SIGN+=(--timestamp)
codesign "${SIGN[@]}" "$FW/Versions/B/Autoupdate" >/dev/null
codesign "${SIGN[@]}" "$FW/Versions/B/Updater.app" >/dev/null
codesign "${SIGN[@]}" "$FW" >/dev/null
codesign "${SIGN[@]}" "$APP/Contents/MacOS/pusmogo-mcp" >/dev/null
codesign "${SIGN[@]}" --entitlements "$ROOT/app/Pusmogo.entitlements" "$APP" >/dev/null
codesign --verify --deep --strict "$APP"
(( $(du -sk "$APP" | cut -f1) < 10240 )) || { echo "the app passes 10 MB" >&2; exit 1; }

echo "✓ $APP ($(du -sh "$APP" | cut -f1))"
