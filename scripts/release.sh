#!/usr/bin/env bash
# Builds one signed, notarised release into build/release/: the app in a disk image and
# the update list (appcast.xml) that installed copies read. Publishes nothing.
#   PUSMOGO_SIGN_ID="Developer ID Application: ..." PUSMOGO_NOTARY=<keychain profile> \
#     ./scripts/release.sh
# The version is app/Info.plist (CFBundleShortVersionString, CFBundleVersion); raise both
# before a release: Sparkle compares CFBundleVersion. One-time setup of the profile:
#   xcrun notarytool store-credentials <profile> --key <AuthKey.p8> --key-id <id> --issuer <id>
# Updates are signed with the key in the login keychain (Sparkle generate_keys --account
# pusmogo); its public half is in Info.plist. Then upload both files to a GitHub release
# tagged v<version>. Needs create-dmg (see make-dmg.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/build/Pusmogo.app"
OUT="$ROOT/build/release"
[[ "${PUSMOGO_SIGN_ID:-}" == "Developer ID Application:"* ]] || { echo "set PUSMOGO_SIGN_ID to a Developer ID Application identity" >&2; exit 2; }
[[ -n "${PUSMOGO_NOTARY:-}" ]] || { echo "set PUSMOGO_NOTARY to a notarytool keychain profile" >&2; exit 2; }
[[ -z "$(git -C "$ROOT" status --porcelain)" ]] || { echo "commit or remove local changes first" >&2; exit 1; }
VERSION="$(plutil -extract CFBundleShortVersionString raw "$ROOT/app/Info.plist")"

"$ROOT/scripts/build.sh" --whisper
[[ "$(lipo -archs "$APP/Contents/MacOS/Pusmogo")" == "arm64" ]] || { echo "expected an arm64 build" >&2; exit 1; }
codesign -dvv "$APP" 2>&1 | grep -q "Authority=Developer ID Application" || { echo "not signed with Developer ID" >&2; exit 1; }

# Fails unless Apple accepts the file.
notarise() {
  local log
  log="$(xcrun notarytool submit "$1" --keychain-profile "$PUSMOGO_NOTARY" --wait --output-format json)"
  echo "$log"
  [[ "$(plutil -extract status raw -o - - <<<"$log")" == "Accepted" ]]
}

rm -rf "$OUT"
mkdir -p "$OUT"
echo "→ notarise the app"
ditto -c -k --keepParent "$APP" "$OUT/app.zip"
notarise "$OUT/app.zip"
rm "$OUT/app.zip"
xcrun stapler staple "$APP"

echo "→ disk image"
"$ROOT/scripts/make-dmg.sh" --no-build
DMG="$OUT/Pusmogo-$VERSION.dmg"
mv "$ROOT/build/Pusmogo.dmg" "$DMG"
codesign --force --timestamp --sign "$PUSMOGO_SIGN_ID" "$DMG"
notarise "$DMG"
xcrun stapler staple "$DMG"
spctl --assess --type open --context context:primary-signature --verbose "$DMG"
spctl --assess --type execute --verbose "$APP"

echo "→ update list"
APPCAST="$(find "$ROOT/app/.build/artifacts" -type f -name generate_appcast | head -1)"
"$APPCAST" --account pusmogo \
  --download-url-prefix "https://github.com/clausis-ai/pusmogo/releases/download/v$VERSION/" "$OUT"
grep -q 'sparkle:edSignature=' "$OUT/appcast.xml" || { echo "the update list is not signed" >&2; exit 1; }
ls -la "$OUT"
echo "✓ upload Pusmogo-$VERSION.dmg and appcast.xml to the release v$VERSION"
