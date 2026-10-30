import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { CaptureReport, Entry, EntrySummary, Problem, Stats, Status, Template } from "./types";
import { LockScreen } from "./components/LockScreen";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { IN_TAURI } from "./api";
import { report } from "./lib/log";
import { Sidebar, type View } from "./components/Sidebar";
import { EntryEditor } from "./components/EntryEditor";
import { OnThisDay } from "./components/OnThisDay";
import { CalendarView } from "./components/CalendarView";
import { MemoryView } from "./components/MemoryView";
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
  const [problems, setProblems] = useState<Problem[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [locked, setLocked] = useState<boolean | null>(null); // null = not yet known
  const [idleMinutes, setIdleMinutes] = useState(5);
  const [toast, setToast] = useState<{ text: string; action?: { label: string; run: () => void } } | null>(null);
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
      setEntries(await api.list({ journal: journal ?? undefined, tag: tag ?? undefined, starred: starredOnly || undefined, limit: 500, includeDrafts: true }));
    }
  }, [query, journal, tag, starredOnly]);

  const refreshMeta = useCallback(async () => {
    const [j, t, o, pr, st, tp] = await Promise.all([api.journals(), api.tags(), api.onThisDay(todayISO()), api.problems(), api.stats(todayISO()), api.templates()]);
    setJournals(j);
    setTags(t);
    setOtd(o);
    setProblems(pr);
    setStats(st);
    setTemplates(tp);
  }, []);

  useEffect(() => { api.status().then(setStatus); }, []);

  // Lock: locked on launch when a passcode exists, and again after idle.
  const refreshLock = useCallback(async () => {
    const li = await api.lockInfo();
    setIdleMinutes(li.idle_minutes);
    return li;
  }, []);
  useEffect(() => {
    refreshLock().then((li) => setLocked(li.enabled));
  }, [refreshLock]);
  useEffect(() => {
    if (locked !== false || idleMinutes <= 0) return;
    let last = Date.now();
    const bump = () => { last = Date.now(); };
    const events = ["mousemove", "keydown", "mousedown", "wheel", "touchstart"] as const;
    events.forEach((ev) => window.addEventListener(ev, bump, { passive: true }));
    const t = window.setInterval(async () => {
      if (Date.now() - last < idleMinutes * 60_000) return;
      const li = await api.lockInfo();
      if (li.enabled) setLocked(true);
    }, 15_000);
    return () => { events.forEach((ev) => window.removeEventListener(ev, bump)); window.clearInterval(t); };
  }, [locked, idleMinutes]);

  // Reminder: once a minute, if enabled and no entry today at the set time.
  useEffect(() => {
    if (!status?.root) return;
    let firedFor = "";
    const tick = async () => {
      const r = await api.getReminder();
      if (!r.enabled) return;
      const now = new Date();
      const hhmm = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
      const key = `${todayISO()} ${r.time}`;
      if (hhmm !== r.time || firedFor === key) return;
      firedFor = key;
      const rows = await api.onDate(todayISO());
      if (rows.length > 0) return;
      // Draft the day first so the reminder can point at something to review.
      const cap = await captureIfConfigured();
      if (!IN_TAURI) { console.info("reminder would fire"); return; }
      let ok = await isPermissionGranted();
      if (!ok) ok = (await requestPermission()) === "granted";
      report("warn", `reminder ${r.time}: permission=${ok} draft=${!!cap?.draft}`);
      if (ok) sendNotification({ title: "Dayfile", body: cap?.draft ? "Your day is drafted. Open Dayfile to add a line and keep it." : "No entry yet today. A line is enough." });
    };
    const t = window.setInterval(tick, 60_000);
    return () => window.clearInterval(t);
  }, [status?.root]);

  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(null), 8000);
    return () => window.clearTimeout(t);
  }, [toast]);

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
        if (s.added || s.updated || s.removed || s.skipped) {
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

  // Capture once per launch, after unlock, when sources are set up.
  const captured = useRef(false);
  useEffect(() => {
    if (captured.current || !status?.root || locked !== false) return;
    captured.current = true;
    captureIfConfigured().then((r) => {
      if (!r || r.added === 0 || !r.draft) return;
      const id = r.draft.uuid;
      refreshList();
      setToast({ text: `Today's draft: ${describe(r)}.`, action: { label: "Review", run: () => { setToast(null); openEntry(id); } } });
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status?.root, locked]);

  // Dev hooks: VITE_OPEN_FIRST=1 opens the newest entry on launch; VITE_SELFTEST=1 runs src/selftest.ts.
  const devHooked = useRef(false);
  useEffect(() => {
    if (devHooked.current || !status?.root) return;
    devHooked.current = true;
    if (import.meta.env.VITE_SELFTEST) import("./selftest").then((m) => m.runSelfTest(openEntry));
    else if (import.meta.env.VITE_OPEN_FIRST && entries[0]) openEntry(entries[0].uuid);
  }, [status?.root, entries, openEntry]);

  const newEntry = useCallback(async (date?: string, template?: string) => {
    const created = date ? `${date}T12:00:00${localRfc3339().slice(19)}` : undefined;
    const draft = { body: "", journal: journal ?? undefined, created };
    const e = template ? await api.createFromTemplate(template, draft) : await api.create(draft);
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
    setEntries((rows) => rows.map((r) => (r.uuid === e.uuid ? { ...r, title: titleOf(e.body), tags: e.tags, starred: e.starred, journal: e.journal, draft: e.status === "draft" } : r)));
    refreshMeta();
  }, [refreshMeta]);

  const onDeleted = useCallback(async (id: string) => {
    setSelected((cur) => (cur?.uuid === id ? null : cur));
    await refreshList();
    refreshMeta();
    setToast({
      text: "Moved to Trash. Restorable for 30 days in Settings.",
      action: {
        label: "Undo",
        run: async () => {
          const e = await api.restore(id);
          setToast(null);
          setSelected(e);
          await refreshList();
          refreshMeta();
        },
      },
    });
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

  if (!status || locked === null) return <div className="loading" />;
  if (locked) return <LockScreen onUnlock={() => setLocked(false)} />;
  if (!status.root) return <Onboarding status={status} onReady={setStatus} />;

  const showOtd = selected && selected.created.slice(0, 10) === todayISO();
  const drafts = entries.filter((e) => e.draft);
  const captureNow = async () => {
    const cfg = await api.getCapture();
    if (!cfg.git.length && !cfg.calendars.length && !cfg.photos.length) {
      setView("settings");
      setToast({ text: "Add a git folder, calendar or photo folder under Capture first." });
      return;
    }
    const r = await api.captureDay();
    await refreshList();
    if (r.draft) openEntry(r.draft.uuid);
    setToast({ text: r.added ? `Captured ${describe(r)}.` : "Nothing new since the last capture." + (r.errors.length ? ` ${r.errors.join(" · ")}` : "") });
  };

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
        onNew={(template) => newEntry(undefined, template)}
        templates={templates}
        searchRef={searchRef}
        problemCount={problems.length}
      />
      <main className="main">
        {view === "settings" && (
          <Settings
            status={status}
            theme={theme}
            onTheme={setTheme}
            onStatus={setStatus}
            onImported={() => { refreshList(); refreshMeta(); }}
            problems={problems}
            onReindexed={refreshMeta}
            onRestored={async (e) => { setSelected(e); setView("timeline"); await refreshList(); refreshMeta(); }}
            onLockChanged={refreshLock}
            onLockNow={() => setLocked(true)}
            onCaptured={() => refreshList()}
          />
        )}
        {view === "memory" && <MemoryView onOpen={openEntry} />}
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
              <div className="btn-row center">
                <button className="primary" onClick={() => newEntry()}>New entry</button>
                <button onClick={captureNow}>Capture today</button>
              </div>
              {drafts.length > 0 && (
                <p><button className="link" onClick={() => openEntry(drafts[0].uuid)}>{drafts.length === 1 ? "1 drafted day to review" : `${drafts.length} drafted days to review`}</button></p>
              )}
              {stats && stats.total > 0 && (
                <p className="stats-line">
                  {stats.streak_days > 0 ? `${stats.streak_days}-day streak` : "No entry yet today"}
                  {` · ${stats.this_month} this month · ${stats.total} ${stats.total === 1 ? "entry" : "entries"} · ${stats.words.toLocaleString()} ${stats.words === 1 ? "word" : "words"}`}
                </p>
              )}
            </div>
          </div>
        )}
      </main>
      {toast && (
        <div className="toast">
          <span>{toast.text}</span>
          {toast.action && <button className="link" onClick={toast.action.run}>{toast.action.label}</button>}
        </div>
      )}
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
    l = l.replace(/^[#>\-*+ ]+/, "").replace(/^\d+[.)] /, "").replace(/^[*_`]+|[*_`]+$/g, "").trim();
    if (l) return l;
  }
  return "";
}

async function captureIfConfigured(): Promise<CaptureReport | null> {
  try {
    const cfg = await api.getCapture();
    if (!cfg.git.length && !cfg.calendars.length && !cfg.photos.length) return null;
    return await api.captureDay();
  } catch (e) {
    report("warn", `capture failed: ${e}`);
    return null;
  }
}

function describe(r: CaptureReport) {
  const n = (k: number, one: string, many: string) => (k === 1 ? `1 ${one}` : `${k} ${many}`);
  return [r.commits && n(r.commits, "commit", "commits"), r.events && n(r.events, "event", "events"), r.photos && n(r.photos, "photo", "photos")].filter(Boolean).join(", ");
}

function greeting() {
  const h = new Date().getHours();
  return h < 5 ? "Still up?" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
}
