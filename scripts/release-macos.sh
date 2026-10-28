#!/usr/bin/env bash
# Build, sign, notarize and upload the macOS bundles for a tag.
#
#   scripts/release-macos.sh v0.1.0
#
# Signing uses the Developer ID certificate in the login keychain.
# Notarization runs only if a notarytool keychain profile named "journal"
# exists (create it once with:
#   xcrun notarytool store-credentials journal --apple-id <email> --team-id <team> --password <app-specific-password>
# ); otherwise the DMGs are uploaded signed but un-notarized.
set -euo pipefail

TAG="${1:?usage: $0 <tag>}"
IDENTITY="${APPLE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning | grep -o '"Developer ID Application: [^"]*"' | head -1 | tr -d '"')}"
PROFILE="${NOTARY_PROFILE:-journal}"
REPO="${GITHUB_REPO:-sjh9714/journal}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

[ -n "$IDENTITY" ] || { echo "no Developer ID Application identity in keychain" >&2; exit 1; }
echo "signing identity: $IDENTITY"

cd "$ROOT/apps/desktop"
DMGS=()
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "$target" >/dev/null 2>&1 || true
  APPLE_SIGNING_IDENTITY="$IDENTITY" npm run -s tauri build -- --target "$target" >/dev/null
  dmg=$(ls "$ROOT/target/$target/release/bundle/dmg/"*.dmg | head -1)
  codesign --verify --strict "$ROOT/target/$target/release/bundle/macos/Journal.app"
  DMGS+=("$dmg")
done

if xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1; then
  for dmg in "${DMGS[@]}"; do
    echo "notarizing $(basename "$dmg")"
    xcrun notarytool submit "$dmg" --keychain-profile "$PROFILE" --wait
    xcrun stapler staple "$dmg"
    spctl -a -vv -t open --context context:primary-signature "$dmg" 2>&1 | tail -1
  done
else
  echo "notarytool profile '$PROFILE' not found: uploading signed, un-notarized DMGs" >&2
fi

gh release upload "$TAG" --repo "$REPO" --clobber "${DMGS[@]}"
echo "uploaded: ${DMGS[*]##*/}"
