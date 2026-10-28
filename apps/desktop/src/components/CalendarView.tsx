import { useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { api } from "../api";
import type { EntrySummary } from "../types";
import { monthLabel, todayISO } from "../lib/dates";

export function CalendarView({ onOpen, onNewOn }: { onOpen: (id: string) => void; onNewOn: (date: string) => void }) {
  const today = todayISO();
  const [cursor, setCursor] = useState(today.slice(0, 7));
  const [rows, setRows] = useState<EntrySummary[]>([]);
  const [picked, setPicked] = useState<string | null>(today);

  const [y, m] = cursor.split("-").map(Number);
  const first = new Date(y, m - 1, 1);
  const daysInMonth = new Date(y, m, 0).getDate();
  const lead = first.getDay();

  useEffect(() => {
    const from = `${cursor}-01`;
    const to = `${cursor}-${String(daysInMonth).padStart(2, "0")}`;
    api.list({ from, to, ascending: true }).then(setRows).catch(() => setRows([]));
  }, [cursor, daysInMonth]);

  const byDay = useMemo(() => {
    const map = new Map<string, EntrySummary[]>();
    for (const r of rows) {
      if (!map.has(r.date)) map.set(r.date, []);
      map.get(r.date)!.push(r);
    }
    return map;
  }, [rows]);

  const shift = (d: number) => {
    const nd = new Date(y, m - 1 + d, 1);
    setCursor(`${nd.getFullYear()}-${String(nd.getMonth() + 1).padStart(2, "0")}`);
  };
  const dayRows = picked ? byDay.get(picked) ?? [] : [];

  return (
    <div className="calendar">
      <div className="cal-head">
        <button className="icon-btn" onClick={() => shift(-1)}><ChevronLeft size={16} /></button>
        <h3>{monthLabel(cursor)}</h3>
        <button className="icon-btn" onClick={() => shift(1)}><ChevronRight size={16} /></button>
        <button className="link" onClick={() => { setCursor(today.slice(0, 7)); setPicked(today); }}>Today</button>
      </div>
      <div className="cal-grid">
        {["S", "M", "T", "W", "T", "F", "S"].map((d, i) => <div key={i} className="cal-dow">{d}</div>)}
        {Array.from({ length: lead }).map((_, i) => <div key={`l${i}`} />)}
        {Array.from({ length: daysInMonth }).map((_, i) => {
          const date = `${cursor}-${String(i + 1).padStart(2, "0")}`;
          const n = byDay.get(date)?.length ?? 0;
          const cls = ["cal-day", date === today ? "today" : "", date === picked ? "picked" : "", n ? "has" : ""].join(" ");
          return (
            <button key={date} className={cls} onClick={() => setPicked(date)} onDoubleClick={() => (n ? onOpen(byDay.get(date)![0].uuid) : onNewOn(date))}>
              <span>{i + 1}</span>
              {n > 0 && <span className="dots">{Array.from({ length: Math.min(n, 3) }).map((_, k) => <i key={k} />)}</span>}
            </button>
          );
        })}
      </div>
      {picked && (
        <div className="cal-list">
          <h5>{picked}</h5>
          {dayRows.length === 0 && (
            <button className="link" onClick={() => onNewOn(picked)}>Write an entry for this day</button>
          )}
          {dayRows.map((r) => (
            <button key={r.uuid} className="row" onClick={() => onOpen(r.uuid)}>
              <div className="row-main"><div className="row-title">{r.title || <em className="dim">Untitled</em>}</div></div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
