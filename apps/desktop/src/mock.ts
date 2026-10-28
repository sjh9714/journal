// In-memory stand-in used when the UI runs in a plain browser (vite dev)
// instead of inside Tauri. Lets the interface be developed and screenshotted
// without the native shell. Never used in the packaged app.
import type { Entry, EntryPatch, EntrySummary, Filter, NewEntry, Status } from "./types";

const entries = new Map<string, Entry>();
let root: string | null = "/Users/demo/Journal";

function uuid() {
  return crypto.randomUUID();
}
function titleOf(body: string) {
  for (const raw of body.split("\n")) {
    let l = raw.trim();
    if (!l) continue;
    if (l.startsWith("![")) {
      const alt = l.slice(2).split("]")[0].trim();
      if (!alt) continue;
      l = alt;
    }
    if (/^[-*_ ]+$/.test(l)) continue;
    l = l.replace(/^[#>\-*+ ]+/, "").replace(/^[\d.)]+\s*/, "").replace(/^[*_`]+|[*_`]+$/g, "").trim();
    if (l) return l;
  }
  return "";
}
function summary(e: Entry): EntrySummary {
  return { uuid: e.uuid, created: e.created, date: e.created.slice(0, 10), journal: e.journal, tags: e.tags, starred: e.starred, title: titleOf(e.body), path: e.path };
}
function normalize(b: string) {
  const t = b.replace(/[\n\r]+$/, "");
  return t ? `${t}\n` : "";
}
function seed() {
  const mk = (created: string, body: string, tags: string[], starred = false, journal = "default") => {
    const id = uuid();
    entries.set(id, { uuid: id, created, timezone: "Asia/Seoul", journal, tags, starred, body: normalize(body), path: `entries/${created.slice(0, 4)}/${created.slice(0, 10)}-${id.slice(0, 8)}.md`, location: null, weather: null });
  };
  const y = new Date().getFullYear();
  const today = new Date().toISOString().slice(0, 10);
  mk(`${y - 2}-${today.slice(5)}T09:12:00+09:00`, "# Two years ago today\n\nStarted the morning run habit. Legs hurt, mind clear.", ["running"], true);
  mk(`${y - 1}-${today.slice(5)}T22:40:00+09:00`, "Quiet evening. Finished the book about rivers.\n\nNeed to call mom tomorrow.", ["reading", "family"]);
  mk(`${y}-01-15T08:00:00+09:00`, "# New year planning\n\n- ship the journal app\n- run 500 km\n- read 24 books", ["planning"], false, "work");
  mk(`${y}-03-03T19:30:00+09:00`, "Standup notes: importer shipped, MCP next.", ["work"], false, "work");
  mk(`${y}-02-02T12:00:00+09:00`, "A tagless note.", []);
  mk(`${today}T07:45:00+09:00`, "# Today\n\nCoffee, then writing. The light through the window is good this morning.", ["morning"]);
}
seed();

function filterList(f: Filter): EntrySummary[] {
  let rows = [...entries.values()].map(summary);
  if (f.journal) rows = rows.filter((r) => r.journal === f.journal);
  if (f.tag) rows = rows.filter((r) => r.tags.includes(f.tag!));
  if (f.from) rows = rows.filter((r) => r.date >= f.from!);
  if (f.to) rows = rows.filter((r) => r.date <= f.to!);
  if (f.starred != null) rows = rows.filter((r) => r.starred === f.starred);
  rows.sort((a, b) => (a.created < b.created ? 1 : -1));
  if (f.ascending) rows.reverse();
  const off = f.offset ?? 0;
  return f.limit != null ? rows.slice(off, off + f.limit) : rows.slice(off);
}

export const mockInvoke = async (cmd: string, args: Record<string, unknown> = {}): Promise<unknown> => {
  await new Promise((r) => setTimeout(r, 15));
  const status = (): Status => ({ root, index_path: root ? "/Users/demo/Library/Application Support/journal/abc/index.db" : null, entries: entries.size, default_root: "/Users/demo/Journal" });
  switch (cmd) {
    case "get_status":
      return status();
    case "open_root":
      root = args.path as string;
      return status();
    case "list_entries":
      return filterList((args.filter as Filter) ?? {});
    case "get_entry": {
      const id = args.id as string;
      const e = [...entries.values()].find((x) => x.uuid.startsWith(id));
      if (!e) throw new Error(`entry not found: ${id}`);
      return e;
    }
    case "create_entry": {
      const n = args.entry as NewEntry;
      const id = uuid();
      const created = n.created ?? new Date().toISOString();
      const e: Entry = { uuid: id, created, timezone: "Asia/Seoul", journal: n.journal ?? "default", tags: n.tags ?? [], starred: !!n.starred, body: normalize(n.body), path: `entries/${created.slice(0, 4)}/${created.slice(0, 10)}-${id.slice(0, 8)}.md`, location: n.location ?? null, weather: n.weather ?? null };
      entries.set(id, e);
      return e;
    }
    case "update_entry": {
      const e = (await mockInvoke("get_entry", { id: args.id })) as Entry;
      const p = args.patch as EntryPatch;
      if (p.body != null) e.body = normalize(p.body);
      if (p.tags) e.tags = [...new Set(p.tags.map((t) => t.replace(/^#/, "").trim()).filter(Boolean))].sort();
      if (p.starred != null) e.starred = p.starred;
      if (p.journal) e.journal = p.journal;
      if ("location" in p) e.location = p.location ?? null;
      if ("weather" in p) e.weather = p.weather ?? null;
      return e;
    }
    case "delete_entry": {
      const e = (await mockInvoke("get_entry", { id: args.id })) as Entry;
      entries.delete(e.uuid);
      return e;
    }
    case "search_entries": {
      const q = (args.query as string).toLowerCase().split(/\s+/).filter(Boolean).map((w) => w.replace(/\*$/, ""));
      return [...entries.values()].filter((e) => q.every((w) => (e.body + " " + e.tags.join(" ")).toLowerCase().includes(w))).map(summary);
    }
    case "on_this_day": {
      const d = args.date as string;
      return filterList({}).filter((r) => r.date.slice(5) === d.slice(5) && r.date.slice(0, 4) !== d.slice(0, 4));
    }
    case "on_date":
      return filterList({ from: args.date as string, to: args.date as string, ascending: true });
    case "list_tags": {
      const m = new Map<string, number>();
      for (const e of entries.values()) for (const t of e.tags) m.set(t, (m.get(t) ?? 0) + 1);
      return [...m.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    }
    case "list_journals": {
      const m = new Map<string, number>();
      for (const e of entries.values()) m.set(e.journal, (m.get(e.journal) ?? 0) + 1);
      return [...m.entries()].sort();
    }
    case "list_problems":
      return [{ path: "entries/2026/2026-03-03-conflict (Dropbox).md", reason: "missing closing ---" }];
    case "reindex":
      return { added: 0, updated: 0, removed: 0, unchanged: entries.size, skipped: 0 };
    case "attach_file":
      return { rel: "attachments/mock.png", link: "../../attachments/mock.png" };
    case "import_dayone_export":
      return { files: ["Journal.json"], entries: 3, created: 3, already_present: 0, attachments: 1, missing_media: 0, journals: ["Journal"] };
    case "entry_abs_path":
      return `${root}/${((await mockInvoke("get_entry", { id: args.id })) as Entry).path}`;
    default:
      throw new Error(`mock: unknown command ${cmd}`);
  }
};
