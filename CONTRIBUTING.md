# Contributing

```sh
(cd apps/desktop && npm install)
scripts/check.sh                  # tests, clippy, web build; what CI runs
git config core.hooksPath .githooks   # clippy before commit, all of it before push
cd apps/desktop && npm run tauri dev  # native app
npx vite                          # UI only, in a browser, with mock data
```

Bug reports with a reproduction (an entry file or a Day One export that
misbehaves) are worth more than feature ideas. Keep PRs to one change.
