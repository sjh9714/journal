#!/usr/bin/env bash
# Everything CI checks, runnable locally. `--ci` skips the desktop crate,
# which needs Tauri's system libraries on Linux runners.
set -euo pipefail
cd "$(dirname "$0")/.."
# Git hooks and GUI clients often run without the login shell's PATH.
command -v cargo >/dev/null || export PATH="$HOME/.cargo/bin:$PATH"

exclude=()
[[ "${1:-}" == "--ci" ]] && exclude=(--exclude journal-desktop)

cargo test --workspace ${exclude[@]+"${exclude[@]}"}
cargo clippy --workspace --all-targets ${exclude[@]+"${exclude[@]}"} -- -D warnings
if [[ "${1:-}" != "--ci" ]]; then
  (cd apps/desktop && npm run build)
fi
