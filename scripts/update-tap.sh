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
mkdir -p "$work/tap/Formula"
cat > "$work/tap/Formula/journal.rb" <<RUBY
class Journal < Formula
  desc "Markdown-file journal: CLI and MCP server for the Journal app"
  homepage "https://github.com/$REPO"
  version "$VERSION"
  license "MIT"

  on_macos do
    on_arm do
      url "$BASE/journal-aarch64-apple-darwin"
      sha256 "$ARM"
    end
    on_intel do
      url "$BASE/journal-x86_64-apple-darwin"
      sha256 "$X64"
    end
  end

  on_linux do
    url "$BASE/journal-x86_64-unknown-linux-gnu"
    sha256 "$LNX"
  end

  def install
    binary = Dir["journal-*"].first
    bin.install binary => "journal"
  end

  test do
    system "#{bin}/journal", "--version"
  end
end
RUBY
cd "$work/tap"
git add Formula/journal.rb
git -c user.name="$(git config --global user.name || echo tap)" -c user.email="$(git config --global user.email || echo tap@users.noreply.github.com)" commit -q -m "journal $VERSION" || { echo "formula unchanged"; exit 0; }
git push -q origin HEAD
echo "tap updated: brew install $TAP_REPO/journal -> $VERSION"
