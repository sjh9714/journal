# Working on this repo as an agent

- One source of truth: `crates/dayfile-core`. CLI (`crates/dayfile-cli`), MCP (`crates/dayfile-cli/src/mcp.rs`) and the desktop app (`apps/desktop/src-tauri`) only call into it. Add behaviour to core first, then expose it on each surface.
- Entries are files; the SQLite index is disposable. Never make a feature depend on index state that cannot be rebuilt from files. See `docs/format.md`.
- `scripts/check.sh` (tests, clippy `-D warnings`, web build; same as CI) must pass before calling anything done.
- The UI can be driven in a plain browser: `cd apps/desktop && npx vite` serves it with an in-memory mock (`src/mock.ts`). Use that for screenshots; the native shell needs `npm run tauri dev`.
- Native QA (Orca computer use, Korean input, lock, reminder notifications): `docs/qa.md`.
- Keep Day One compatibility in `crates/dayfile-core/src/dayone.rs` idempotent: importing the same export twice must create nothing.
- Do not add dependencies for things the standard library or an existing dependency already does.
