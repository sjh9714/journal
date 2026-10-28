import { useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, CalendarDays, ChevronDown, List, Plus, Search, Settings, Star } from "lucide-react";
import type { EntrySummary, Template } from "../types";
import { monthKey, monthLabel, timeLabel } from "../lib/dates";

export type View = "timeline" | "calendar" | "settings";

interface Props {
  entries: EntrySummary[];
  selected: string | null;
  query: string;
  journals: [string, number][];
  tags: [string, number][];
  journal: string | null;
  tag: string | null;
  starredOnly: boolean;
  view: View;
  onSelect: (id: string) => void;
  onQuery: (q: string) => void;
  onJournal: (j: string | null) => void;
  onTag: (t: string | null) => void;
  onStarred: (s: boolean) => void;
  onView: (v: View) => void;
  onNew: (template?: string) => void;
  templates: Template[];
  searchRef: React.RefObject<HTMLInputElement | null>;
  /** Files the indexer could not parse; shown as a chip that opens Settings. */
  problemCount: number;
}

export function Sidebar(p: Props) {
  const [menu, setMenu] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menu) return;
    const close = (e: MouseEvent) => { if (!menuRef.current?.contains(e.target as Node)) setMenu(false); };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [menu]);
  const groups = useMemo(() => {
    const m = new Map<string, EntrySummary[]>();
    for (const e of p.entries) {
      const k = monthKey(e.date);
      if (!m.has(k)) m.set(k, []);
      m.get(k)!.push(e);
    }
    return [...m.entries()];
  }, [p.entries]);

  return (
    <aside className="sidebar">
      <div className="sidebar-top" data-tauri-drag-region>
        <div className="search">
          <Search size={14} />
          <input ref={p.searchRef} placeholder="Search  ⌘K" value={p.query} onChange={(e) => p.onQuery(e.target.value)} />
        </div>
        <div className="new-wrap" ref={menuRef}>
          <button className="icon-btn" title="New entry  ⌘N" onClick={() => p.onNew()}>
            <Plus size={18} />
          </button>
          {p.templates.length > 0 && (
            <button className="icon-btn tiny" title="New from template" onClick={() => setMenu((m) => !m)}>
              <ChevronDown size={14} />
            </button>
          )}
          {menu && (
            <div className="menu">
              <button onClick={() => { setMenu(false); p.onNew(); }}>Blank entry</button>
              <div className="menu-sep" />
              {p.templates.map((t) => (
                <button key={t.slug} onClick={() => { setMenu(false); p.onNew(t.slug); }}>{t.name}</button>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="viewbar">
        <button className={p.view === "timeline" ? "on" : ""} onClick={() => p.onView("timeline")} title="Timeline">
          <List size={15} />
        </button>
        <button className={p.view === "calendar" ? "on" : ""} onClick={() => p.onView("calendar")} title="Calendar">
          <CalendarDays size={15} />
        </button>
        <button className={p.starredOnly ? "on" : ""} onClick={() => p.onStarred(!p.starredOnly)} title="Starred only">
          <Star size={15} />
        </button>
        <span className="grow" />
        <button className={p.view === "settings" ? "on" : ""} onClick={() => p.onView("settings")} title="Settings">
          <Settings size={15} />
        </button>
      </div>

      {p.journals.length > 1 && (
        <div className="chips">
          <button className={!p.journal ? "chip on" : "chip"} onClick={() => p.onJournal(null)}>All</button>
          {p.journals.map(([j, n]) => (
            <button key={j} className={p.journal === j ? "chip on" : "chip"} onClick={() => p.onJournal(p.journal === j ? null : j)}>
              {j} <span className="dim">{n}</span>
            </button>
          ))}
        </div>
      )}

      {p.tags.length > 0 && (
        <div className="chips tags-row">
          {p.tags.slice(0, 24).map(([t, n]) => (
            <button key={t} className={p.tag === t ? "chip on" : "chip"} onClick={() => p.onTag(p.tag === t ? null : t)} title={`${n} entries`}>
              #{t}
            </button>
          ))}
        </div>
      )}

      <div className="list">
        {p.entries.length === 0 && <div className="empty">{p.query || p.journal || p.tag || p.starredOnly ? "No entries match." : "No entries yet. Press ⌘N to write."}</div>}
        {groups.map(([k, rows]) => (
          <section key={k}>
            <h4>{monthLabel(k)}</h4>
            {rows.map((e) => (
              <button key={e.uuid} className={e.uuid === p.selected ? "row on" : "row"} onClick={() => p.onSelect(e.uuid)}>
                <div className="row-date">
                  <span className="day">{e.date.slice(8)}</span>
                  <span className="time">{timeLabel(e.created)}</span>
                </div>
                <div className="row-main">
                  <div className="row-title">
                    {e.starred && <Star size={11} className="star" />}
                    {e.title || <em className="dim">Untitled</em>}
                  </div>
                  {e.tags.length > 0 && <div className="row-tags">{e.tags.map((t) => `#${t}`).join(" ")}</div>}
                </div>
              </button>
            ))}
          </section>
        ))}
      </div>
      {p.problemCount > 0 && (
        <button className="problem-chip" onClick={() => p.onView("settings")} title="Some files could not be read. Click for details.">
          <AlertTriangle size={13} />
          {p.problemCount === 1 ? "1 file skipped" : `${p.problemCount} files skipped`}
        </button>
      )}
    </aside>
  );
}
