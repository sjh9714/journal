# Contributing

```sh
cargo test --workspace            # core, importer, MCP over stdio
cd apps/desktop && npm install && npm run build
npm run tauri dev                 # native app
npx vite                          # UI only, in a browser, with mock data
```

Bug reports with a reproduction (an entry file or a Day One export that
misbehaves) are worth more than feature ideas. Keep PRs to one change.
