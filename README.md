# Journal

A journal that is just a folder of Markdown files, with an app that feels
like Day One and an MCP server so your agent can read and write it.

- **Your files.** One `.md` per entry with YAML frontmatter; photos in a
  content-addressed folder. Sync with iCloud Drive, Dropbox, Syncthing, git.
  Open with any editor. See [`docs/format.md`](docs/format.md).
- **Day One import.** Export → JSON in Day One, import the zip. Entries,
  photos, tags, location, weather and stars come across. Re-importing never
  duplicates.
- **On This Day**, full-text search (FTS5, works for Korean/Japanese/CJK),
  tags, multiple journals, calendar view, dark mode.
- **Agents.** `journal mcp serve` exposes 11 tools over MCP; the CLI does
  the same from a shell. See [`docs/mcp.md`](docs/mcp.md).
- **No account, no server, no telemetry.** Optional location/weather uses the
  OS location service and Open-Meteo (keyless).

## Install

Desktop app: download the build for macOS, Windows or Linux from Releases.

CLI (also the MCP server):

```sh
cargo install --path crates/journal-cli
journal init                       # creates ~/Journal
journal add "First entry" -t life
journal list
journal search first
journal on-this-day
```

## Let your agent in

```sh
claude mcp add journal -- journal mcp serve
```

Then: "add a line to today's journal about the deploy", "what did I write
about the move last year?", "summarise my week". `--read-only` if you only
want it to read.

## Import from Day One

Day One → Export → JSON. Then in the app: Settings → Import Day One export,
or:

```sh
journal import dayone ~/Downloads/Export.zip
```

## Build from source

```sh
cargo test --workspace
cd apps/desktop && npm install && npm run tauri build
```

## Status

1.0 scope: desktop app (macOS, Windows, Linux), CLI, MCP, Day One import.
Not yet: mobile, built-in sync service, PDF export, encryption at rest.

MIT. "Day One" is a trademark of Automattic; this project only reads its
export format.
