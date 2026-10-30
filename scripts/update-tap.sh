#!/usr/bin/env bash
# Regenerate the Homebrew formula in sjh9714/homebrew-tap from a published release.
#   scripts/update-tap.sh v0.1.0
set -euo pipefail
TAG="${1:?usage: $0 <tag>}"
VERSION="${TAG#v}"
REPO="${GITHUB_REPO:-sjh9714/dayfile}"
TAP_REPO="${TAP_REPO:-sjh9714/homebrew-tap}"
BASE="https://github.com/$REPO/releases/download/$TAG"
work=$(mktemp -d)
sha() { curl -sSL "$BASE/$1" -o "$work/$1" && shasum -a 256 "$work/$1" | cut -d' ' -f1; }
ARM=$(sha dayfile-aarch64-apple-darwin)
X64=$(sha dayfile-x86_64-apple-darwin)
LNX=$(sha dayfile-x86_64-unknown-linux-gnu)
git clone -q "https://github.com/$TAP_REPO.git" "$work/tap"
# A cask, not a formula: casks install prebuilt binaries without needing
# Xcode Command Line Tools, which a formula with no bottle insists on.
mkdir -p "$work/tap/Casks"; rm -f "$work/tap/Formula/journal.rb"
cat > "$work/tap/Casks/dayfile.rb" <<RUBY
cask "dayfile" do
  version "$VERSION"

  on_arm do
    url "$BASE/dayfile-aarch64-apple-darwin"
    sha256 "$ARM"
    binary "dayfile-aarch64-apple-darwin", target: "dayfile"
  end
  on_intel do
    url "$BASE/dayfile-x86_64-apple-darwin"
    sha256 "$X64"
    binary "dayfile-x86_64-apple-darwin", target: "dayfile"
  end

  name "Dayfile CLI"
  desc "Markdown-file journal: CLI and MCP server for the Dayfile app"
  homepage "https://github.com/$REPO"

  livecheck do
    url :url
    strategy :github_latest
  end
end
RUBY
cd "$work/tap"
git add -A
git -c user.name="$(git config --global user.name || echo tap)" -c user.email="$(git config --global user.email || echo tap@users.noreply.github.com)" commit -q -m "dayfile $VERSION" || { echo "formula unchanged"; exit 0; }
git push -q origin HEAD
echo "tap updated: brew install --cask sjh9714/tap/dayfile -> $VERSION"
