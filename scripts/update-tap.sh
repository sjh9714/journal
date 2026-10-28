#!/usr/bin/env bash
# Regenerate the Homebrew formula in sjh9714/homebrew-tap from a published release.
#   scripts/update-tap.sh v0.1.0
set -euo pipefail
TAG="${1:?usage: $0 <tag>}"
VERSION="${TAG#v}"
REPO="${GITHUB_REPO:-sjh9714/journal}"
TAP_REPO="${TAP_REPO:-sjh9714/homebrew-tap}"
BASE="https://github.com/$REPO/releases/download/$TAG"
work=$(mktemp -d)
sha() { curl -sSL "$BASE/$1" -o "$work/$1" && shasum -a 256 "$work/$1" | cut -d' ' -f1; }
ARM=$(sha journal-aarch64-apple-darwin)
X64=$(sha journal-x86_64-apple-darwin)
LNX=$(sha journal-x86_64-unknown-linux-gnu)
git clone -q "https://github.com/$TAP_REPO.git" "$work/tap"
# A cask, not a formula: casks install prebuilt binaries without needing
# Xcode Command Line Tools, which a formula with no bottle insists on.
mkdir -p "$work/tap/Casks"; rm -f "$work/tap/Formula/journal.rb"
cat > "$work/tap/Casks/journal.rb" <<RUBY
cask "journal" do
  version "$VERSION"

  on_arm do
    url "$BASE/journal-aarch64-apple-darwin"
    sha256 "$ARM"
    binary "journal-aarch64-apple-darwin", target: "journal"
  end
  on_intel do
    url "$BASE/journal-x86_64-apple-darwin"
    sha256 "$X64"
    binary "journal-x86_64-apple-darwin", target: "journal"
  end

  name "Journal CLI"
  desc "Markdown-file journal: CLI and MCP server for the Journal app"
  homepage "https://github.com/$REPO"

  livecheck do
    url :url
    strategy :github_latest
  end
end
RUBY
cd "$work/tap"
git add -A
git -c user.name="$(git config --global user.name || echo tap)" -c user.email="$(git config --global user.email || echo tap@users.noreply.github.com)" commit -q -m "journal $VERSION" || { echo "formula unchanged"; exit 0; }
git push -q origin HEAD
echo "tap updated: brew install --cask sjh9714/tap/journal -> $VERSION"
