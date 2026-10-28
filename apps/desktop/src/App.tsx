import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { Entry, EntrySummary, Status } from "./types";
import { Sidebar, type View } from "./components/Sidebar";
import { EntryEditor } from "./components/EntryEditor";
import { OnThisDay } from "./components/OnThisDay";
import { CalendarView } from "./components/CalendarView";
import { Settings, type Theme } from "./components/Settings";
import { Onboarding } from "./components/Onboarding";
import { localRfc3339, todayISO } from "./lib/dates";
import { captureContext } from "./lib/enrich";
import "./styles.css";

function loadTheme(): Theme {
  try {
    return (localStorage.getItem("journal.theme") as Theme) || "system";
  } catch {
    return "system";
  }
}

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [entries, setEntries] = useState<EntrySummary[]>([]);
  const [selected, setSelected] = useState<Entry | null>(null);
  const [otd, setOtd] = useState<EntrySummary[]>([]);
  const [journals, setJournals] = useState<[string, number][]>([]);
  const [tags, setTags] = useState<[string, number][]>([]);
  const [query, setQuery] = useState("");
  const [journal, setJournal] = useState<string | null>(null);
  const [tag, setTag] = useState<string | null>(null);
  const [starredOnly, setStarredOnly] = useState(false);
  const [view, setView] = useState<View>("timeline");
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const searchRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try { localStorage.setItem("journal.theme", theme); } catch { /* ignore */ }
  }, [theme]);

  const refreshList = useCallback(async () => {
    if (query.trim()) {
      setEntries(await api.search(query.trim(), 200));
    } else {
      setEntries(await api.list({ journal: journal ?? undefined, tag: tag ?? undefined, starred: starredOnly || undefined, limit: 500 }));
    }
  }, [query, journal, tag, starredOnly]);

  const refreshMeta = useCallback(async () => {
    const [j, t, o] = await Promise.all([api.journals(), api.tags(), api.onThisDay(todayISO())]);
    setJournals(j);
    setTags(t);
    setOtd(o);
  }, []);

  useEffect(() => { api.status().then(setStatus); }, []);

  // Pick up files written by the CLI, an agent over MCP, or a sync tool:
  // incremental reindex on window focus and every 30s.
  useEffect(() => {
    if (!status?.root) return;
    let busy = false;
    const sync = async () => {
      if (busy) return;
      busy = true;
      try {
        const s = await api.reindex(false);
        if (s.added || s.updated || s.removed) {
          await refreshList();
          await refreshMeta();
        }
      } catch { /* ignore */ } finally { busy = false; }
    };
    window.addEventListener("focus", sync);
    const t = window.setInterval(sync, 30_000);
    return () => { window.removeEventListener("focus", sync); window.clearInterval(t); };
  }, [status?.root, refreshList, refreshMeta]);
  useEffect(() => { if (status?.root) { refreshList(); } }, [status?.root, refreshList]);
  useEffect(() => { if (status?.root) { refreshMeta(); } }, [status?.root, refreshMeta]);

  // Bumped on every explicit open so the editor reloads even for the same entry
  // (e.g. after an agent appended to it over MCP).
  const [openSeq, setOpenSeq] = useState(0);
  const openEntry = useCallback(async (id: string) => {
    setSelected(await api.get(id));
    setOpenSeq((n) => n + 1);
    setView("timeline");
  }, []);

  // Dev hooks: VITE_OPEN_FIRST=1 opens the newest entry on launch; VITE_SELFTEST=1 runs src/selftest.ts.
  const devHooked = useRef(false);
  useEffect(() => {
    if (devHooked.current || !status?.root) return;
    devHooked.current = true;
    if (import.meta.env.VITE_SELFTEST) import("./selftest").then((m) => m.runSelfTest(openEntry));
    else if (import.meta.env.VITE_OPEN_FIRST && entries[0]) openEntry(entries[0].uuid);
  }, [status?.root, entries, openEntry]);

  const newEntry = useCallback(async (date?: string) => {
    const created = date ? `${date}T12:00:00${localRfc3339().slice(19)}` : undefined;
    const e = await api.create({ body: "", journal: journal ?? undefined, created });
    setSelected(e);
    setView("timeline");
    await refreshList();
    if (!date) {
      captureContext().then(async (ctx) => {
        if (!ctx.location) return;
        const updated = await api.update(e.uuid, ctx);
        setSelected((cur) => (cur?.uuid === e.uuid ? { ...cur, location: updated.location, weather: updated.weather } : cur));
      });
    }
  }, [journal, refreshList]);

  const onChanged = useCallback((e: Entry) => {
    setSelected((cur) => (cur?.uuid === e.uuid ? e : cur));
    setEntries((rows) => rows.map((r) => (r.uuid === e.uuid ? { ...r, title: titleOf(e.body), tags: e.tags, starred: e.starred, journal: e.journal } : r)));
    refreshMeta();
  }, [refreshMeta]);

  const onDeleted = useCallback(async (id: string) => {
    setSelected((cur) => (cur?.uuid === id ? null : cur));
    await refreshList();
    refreshMeta();
  }, [refreshList, refreshMeta]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      if (e.key === "n") { e.preventDefault(); newEntry(); }
      if (e.key === "k") { e.preventDefault(); setView("timeline"); searchRef.current?.focus(); }
      if (e.key === "t") { e.preventDefault(); api.onDate(todayISO()).then((rows) => (rows[0] ? openEntry(rows[0].uuid) : newEntry())); }
      if (e.key === ",") { e.preventDefault(); setView("settings"); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [newEntry, openEntry]);

  if (!status) return <div className="loading" />;
  if (!status.root) return <Onboarding status={status} onReady={setStatus} />;

  const showOtd = selected && selected.created.slice(0, 10) === todayISO();

  return (
    <div className="app">
      <Sidebar
        entries={entries}
        selected={selected?.uuid ?? null}
        query={query}
        journals={journals}
        tags={tags}
        journal={journal}
        tag={tag}
        starredOnly={starredOnly}
        view={view}
        onSelect={openEntry}
        onQuery={setQuery}
        onJournal={setJournal}
        onTag={setTag}
        onStarred={setStarredOnly}
        onView={setView}
        onNew={() => newEntry()}
        searchRef={searchRef}
      />
      <main className="main">
        {view === "settings" && (
          <Settings status={status} theme={theme} onTheme={setTheme} onStatus={setStatus} onImported={() => { refreshList(); refreshMeta(); }} />
        )}
        {view === "calendar" && <CalendarView onOpen={openEntry} onNewOn={(d) => newEntry(d)} />}
        {view === "timeline" && selected && (
          <>
            {showOtd && <OnThisDay rows={otd} onOpen={openEntry} />}
            <EntryEditor entry={selected} reloadKey={openSeq} journals={journals.map(([j]) => j)} onChanged={onChanged} onDeleted={onDeleted} />
          </>
        )}
        {view === "timeline" && !selected && (
          <div className="home" data-tauri-drag-region>
            <OnThisDay rows={otd} onOpen={openEntry} />
            <div className="home-empty">
              <h2>{greeting()}</h2>
              <p>Pick an entry on the left, or press <kbd>⌘N</kbd> to start today's.</p>
              <button className="primary" onClick={() => newEntry()}>New entry</button>
            </div>
          </div>
        )}
      </main>
    </div>
  );
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

function greeting() {
  const h = new Date().getHours();
  return h < 5 ? "Still up?" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
}
