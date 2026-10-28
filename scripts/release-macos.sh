#!/usr/bin/env bash
# Everything macOS for a release: desktop DMGs (both archs), CLI binaries
# (both archs, Developer ID signed + notarized so Gatekeeper lets them run
# after `brew install`), the .mcpb bundle, and the Homebrew tap.
#
#   scripts/release-macos.sh v0.1.0
#
# Signing uses the Developer ID certificate in the login keychain.
# Notarization runs when a notarytool keychain profile named "journal" exists:
#   xcrun notarytool store-credentials journal --apple-id <email> --team-id <team> --password <app-specific-password>
set -euo pipefail

TAG="${1:?usage: $0 <tag>}"
VERSION="${TAG#v}"
IDENTITY="${APPLE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning | grep -o '"Developer ID Application: [^"]*"' | head -1 | tr -d '"')}"
PROFILE="${NOTARY_PROFILE:-journal}"
REPO="${GITHUB_REPO:-sjh9714/journal}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASE="https://github.com/$REPO/releases/download/$TAG"

[ -n "$IDENTITY" ] || { echo "no Developer ID Application identity in keychain" >&2; exit 1; }
echo "signing identity: $IDENTITY"
NOTARIZE=0
xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1 && NOTARIZE=1 || echo "notarytool profile '$PROFILE' not found: skipping notarization" >&2

notarize() { # file
  [ "$NOTARIZE" = 1 ] || return 0
  echo "notarizing $(basename "$1")"
  xcrun notarytool submit "$1" --keychain-profile "$PROFILE" --wait | grep -E "status:" | tail -1
}

# ---- desktop app ----
cd "$ROOT/apps/desktop"
DMGS=()
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "$target" >/dev/null 2>&1 || true
  APPLE_SIGNING_IDENTITY="$IDENTITY" npm run -s tauri build -- --target "$target" >/dev/null
  dmg=$(ls "$ROOT/target/$target/release/bundle/dmg/"*.dmg | head -1)
  codesign --verify --strict "$ROOT/target/$target/release/bundle/macos/Journal.app"
  notarize "$dmg" && [ "$NOTARIZE" = 1 ] && xcrun stapler staple "$dmg" >/dev/null
  DMGS+=("$dmg")
done

# ---- CLI binaries ----
cd "$ROOT"
work=$(mktemp -d)
CLIS=()
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build -q --release -p journal-cli --target "$target"
  out="$work/journal-$target"
  cp "target/$target/release/journal" "$out"
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$out"
  CLIS+=("$out")
done
if [ "$NOTARIZE" = 1 ]; then
  (cd "$work" && zip -q cli.zip journal-aarch64-apple-darwin journal-x86_64-apple-darwin)
  notarize "$work/cli.zip"   # bare binaries cannot be stapled; the ticket is checked online
fi

# ---- .mcpb bundle (universal mac binary + linux + windows from the release) ----
mkdir -p "$work/mcpb/server"
lipo -create "${CLIS[@]}" -output "$work/mcpb/server/journal-darwin"
codesign --force --options runtime --timestamp --sign "$IDENTITY" "$work/mcpb/server/journal-darwin"
curl -sSL "$BASE/journal-x86_64-unknown-linux-gnu" -o "$work/mcpb/server/journal-linux"
curl -sSL "$BASE/journal-x86_64-pc-windows-msvc.exe" -o "$work/mcpb/server/journal-win.exe"
chmod +x "$work/mcpb/server/"*
sed "s/__VERSION__/$VERSION/g" "$ROOT/packaging/mcpb/manifest.json" > "$work/mcpb/manifest.json"
(cd "$work/mcpb" && zip -qr "../journal-$VERSION.mcpb" manifest.json server)
MCPB="$work/journal-$VERSION.mcpb"

# ---- upload ----
gh release upload "$TAG" --repo "$REPO" --clobber "${DMGS[@]}" "${CLIS[@]}" "$MCPB"
echo "uploaded: $(printf '%s ' "${DMGS[@]##*/}" "${CLIS[@]##*/}" "${MCPB##*/}")"
echo "mcpb sha256: $(shasum -a 256 "$MCPB" | cut -d' ' -f1)"

# ---- Homebrew tap (sha256 of the CLI binaries changed) ----
"$ROOT/scripts/update-tap.sh" "$TAG"
