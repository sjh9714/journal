# Journal

A journal that is just a folder of Markdown files, with an app that feels
like Day One and an MCP server so your agent can read and write it.

![Journal demo: timeline, entry with photo, search, calendar, and an agent reading the journal over MCP](docs/demo.gif)

- **Your files.** One `.md` per entry with YAML frontmatter; photos in a
  content-addressed folder. Sync with iCloud Drive, Dropbox, Syncthing, git.
  Open with any editor. See [`docs/format.md`](docs/format.md).
- **Day One import.** Export → JSON in Day One, import the zip. Entries,
  photos, tags, location, weather and stars come across. Re-importing never
  duplicates.
- **On This Day**, full-text search (FTS5, works for Korean/Japanese/CJK),
  tags, multiple journals, calendar view, dark mode, streak and word counts.
- **Templates** (`templates/*.md`, three included), a **daily reminder**,
  a **passcode lock**, and a **trash** that keeps deleted entries for 30 days.
- **Agents.** `journal mcp serve` exposes 11 tools over MCP; the CLI does
  the same from a shell. See [`docs/mcp.md`](docs/mcp.md).
- **No account, no server, no telemetry.** Optional location/weather uses the
  OS location service when available, otherwise a city-level guess from your
  network connection (ipapi.co), then Open-Meteo (keyless). One toggle turns
  it off.

## Install

Desktop app: download the build for macOS, Windows or Linux from
[Releases](https://github.com/sjh9714/journal/releases). The macOS builds are
signed and notarized.

CLI (also the MCP server):

```sh
brew install --cask sjh9714/tap/journal   # macOS; or grab a binary from Releases
journal init                       # creates ~/Journal (or follows the desktop app's folder)
journal add "First entry" -t life
journal list
journal search first
journal on-this-day
journal add --template gratitude   # see `journal templates`
journal delete <id>                # to the trash; `journal trash restore <id>` undoes it
journal stats
```

## Let your agent in

```sh
claude mcp add journal -- journal mcp serve
```

Claude Desktop: download `journal-0.1.0.mcpb` from Releases and open it.

Then: "add a line to today's journal about the deploy", "what did I write
about the move last year?", "summarise my week". `--read-only` if you only
want it to read.

## Import from Day One

Day One → Export → JSON. Then in the app: Settings → Import Day One export,
or:

```sh
journal import dayone ~/Downloads/Export.zip
```

## On your phone

There is no mobile app yet. Because entries are plain Markdown files, keep
the journal folder in iCloud Drive (or Dropbox, Syncthing) and open it with
any Markdown editor on the phone. On iPhone, [Obsidian](https://obsidian.md)
(free) works well: create a vault, choose "Existing folder", pick the journal
folder in iCloud Drive, and new entries follow the format in
[`docs/format.md`](docs/format.md). iA Writer and 1Writer also open the
folder directly. The desktop app picks the edits up the next time it is
focused.

## Build from source

```sh
cargo test --workspace
cd apps/desktop && npm install && npm run tauri build
```

## Status

Desktop app (macOS, Windows, Linux), CLI, MCP, Day One import, templates,
reminder, lock, trash. Not yet: mobile, built-in sync service, PDF export,
encryption at rest, edit history (keep the folder in git for that).

MIT. "Day One" is a trademark of Automattic; this project only reads its
export format.
