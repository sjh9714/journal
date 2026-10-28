import type { EntrySummary } from "../types";
import { yearsAgo } from "../lib/dates";

export function OnThisDay({ rows, onOpen }: { rows: EntrySummary[]; onOpen: (id: string) => void }) {
  if (rows.length === 0) return null;
  return (
    <div className="otd">
      <h5>On this day</h5>
      <div className="otd-cards">
        {rows.map((r) => (
          <button key={r.uuid} className="otd-card" onClick={() => onOpen(r.uuid)}>
            <span className="otd-when">{yearsAgo(r.date)}</span>
            <span className="otd-title">{r.title || "Untitled"}</span>
            <span className="otd-date">{r.date}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
