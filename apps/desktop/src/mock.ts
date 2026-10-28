// In-memory stand-in used when the UI runs in a plain browser (vite dev)
// instead of inside Tauri. Lets the interface be developed and screenshotted
// without the native shell. Never used in the packaged app.
import type { Entry, EntryPatch, EntrySummary, Filter, NewEntry, Status, Template, TrashedEntry } from "./types";

const entries = new Map<string, Entry>();
const trash = new Map<string, { e: Entry; at: string }>();
let lock: { pass: string; idle: number } | null = null;
let reminder = { enabled: false, time: "21:00" };
let fails = 0;
const templates: Template[] = [
  { slug: "daily", name: "Daily", tags: [], journal: null, body: "# {{weekday}}\n\n**Morning**\n\n\n**Afternoon**\n\n\n**Evening**\n\n" },
  { slug: "gratitude", name: "Gratitude", tags: ["gratitude"], journal: null, body: "# {{weekday}}, {{date}}\n\nThree things I am grateful for today:\n\n1. \n2. \n3. \n" },
  { slug: "weekly-review", name: "Weekly review", tags: ["review"], journal: null, body: "# Week of {{date}}\n\n## What went well\n\n- \n" },
];
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
    l = l.replace(/^[#>\-*+ ]+/, "").replace(/^\d+[.)] /, "").replace(/^[*_`]+|[*_`]+$/g, "").trim();
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
      trash.set(e.uuid, { e, at: new Date().toISOString() });
      return e;
    }
    case "trash_list":
      return [...trash.values()].map(({ e, at }): TrashedEntry => ({ uuid: e.uuid, title: titleOf(e.body), created: e.created, deleted_at: at, trash_path: `.journal/trash/x__${e.path.split("/").pop()}`, journal: e.journal })).sort((a, b) => (a.deleted_at < b.deleted_at ? 1 : -1));
    case "restore_entry": {
      const id = args.id as string;
      const hit = [...trash.values()].find((t) => t.e.uuid.startsWith(id));
      if (!hit) throw new Error(`entry not found: ${id}`);
      trash.delete(hit.e.uuid);
      entries.set(hit.e.uuid, hit.e);
      return hit.e;
    }
    case "empty_trash": {
      const n = trash.size;
      trash.clear();
      return n;
    }
    case "list_templates":
      return templates;
    case "create_from_template": {
      const t = templates.find((x) => x.slug === args.slug);
      if (!t) throw new Error("template not found");
      const now = new Date();
      const body = t.body.replace(/{{date}}/g, now.toISOString().slice(0, 10)).replace(/{{weekday}}/g, now.toLocaleDateString("en-US", { weekday: "long" })).replace(/{{time}}/g, now.toTimeString().slice(0, 5)).replace(/{{year}}/g, String(now.getFullYear()));
      const n = args.entry as NewEntry;
      return mockInvoke("create_entry", { entry: { ...n, body, tags: n.tags?.length ? n.tags : t.tags, journal: n.journal ?? t.journal ?? undefined } });
    }
    case "get_stats": {
      const dates = [...new Set([...entries.values()].map((e) => e.created.slice(0, 10)))].sort().reverse();
      const today = args.today as string;
      let streak = 0;
      if (dates[0] === today || dates[0] === new Date(Date.now() - 86400000).toISOString().slice(0, 10)) {
        streak = 1;
        for (let i = 1; i < dates.length; i++) {
          if ((new Date(dates[i - 1]).getTime() - new Date(dates[i]).getTime()) / 86400000 === 1) streak++;
          else break;
        }
      }
      return { total: entries.size, this_month: dates.filter((d) => d.startsWith(today.slice(0, 7))).length, this_year: dates.filter((d) => d.startsWith(today.slice(0, 4))).length, streak_days: streak, longest_streak: Math.max(streak, 2), words: [...entries.values()].reduce((n, e) => n + e.body.split(/\s+/).filter(Boolean).length, 0), first_entry: dates[dates.length - 1], last_entry: dates[0] };
    }
    case "lock_info":
      return { enabled: !!lock, idle_minutes: lock?.idle ?? 5 };
    case "set_passcode": {
      if (lock && lock.pass !== args.current) throw new Error("current passcode is wrong");
      if (String(args.new).length < 4) throw new Error("passcode must be at least 4 characters");
      lock = { pass: args.new as string, idle: args.idleMinutes as number };
      return { enabled: true, idle_minutes: lock.idle };
    }
    case "set_lock_idle":
      if (lock) lock.idle = args.idleMinutes as number;
      return { enabled: !!lock, idle_minutes: lock?.idle ?? 5 };
    case "clear_passcode":
      if (lock && lock.pass !== args.current) throw new Error("current passcode is wrong");
      lock = null;
      return { enabled: false, idle_minutes: 5 };
    case "verify_passcode": {
      if (!lock || lock.pass === args.pass) { fails = 0; return { ok: true, retry_after: 0 }; }
      fails++;
      if (fails >= 5) { fails = 0; return { ok: false, retry_after: 30 }; }
      return { ok: false, retry_after: 0 };
    }
    case "get_reminder":
      return reminder;
    case "set_reminder":
      reminder = { enabled: args.enabled as boolean, time: args.time as string };
      return reminder;
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
