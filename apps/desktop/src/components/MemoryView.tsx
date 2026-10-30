import { useCallback, useEffect, useState } from "react";
import { Brain, Search } from "lucide-react";
import { api } from "../api";
import type { Fact } from "../types";
import { todayISO } from "../lib/dates";

/** Facts agents recorded from entries, per topic, with when each was true. */
export function MemoryView({ onOpen }: { onOpen: (id: string) => void }) {
  const [topics, setTopics] = useState<[string, number][]>([]);
  const [topic, setTopic] = useState<string | null>(null);
  const [history, setHistory] = useState<Fact[]>([]);
  const [asOf, setAsOf] = useState(todayISO());
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<Fact[] | null>(null);

  const load = useCallback(async () => {
    const t = await api.topics();
    setTopics(t);
    setTopic((cur) => cur ?? t[0]?.[0] ?? null);
  }, []);
  useEffect(() => { load(); }, [load]);
  useEffect(() => { if (topic) api.factHistory(topic).then(setHistory); }, [topic]);
  useEffect(() => {
    const q = query.trim();
    if (!q) return setHits(null);
    const valid = /^\d{4}-\d{2}-\d{2}$/.test(asOf) ? asOf : undefined;
    api.recall({ query: q, asOf: valid }).then(setHits);
  }, [query, asOf]);

  const holds = (f: Fact) => !f.retracted_at && (!f.valid_from || f.valid_from <= asOf) && (!f.valid_to || f.valid_to > asOf);
  const retract = async (f: Fact) => {
    await api.retractFact(f.id);
    if (topic) setHistory(await api.factHistory(topic));
    load();
  };

  if (topics.length === 0) {
    return (
      <div className="memory-empty">
        <Brain size={28} />
        <h2>No memories yet</h2>
        <p>Memory holds facts about your life that your agent picks up from entries: where you live, who people are, what you are working on, and since when.</p>
        <p className="dim">In Claude Code, run the <span className="mono">review_day</span> prompt after capturing a day, or add one yourself with <span className="mono">dayfile remember Home "Lives in Seoul" --since 2024-03-01</span>. Facts are saved as plain files in <span className="mono">memory/</span>.</p>
      </div>
    );
  }

  return (
    <div className="memory">
      <div className="memory-bar">
        <div className="search">
          <Search size={14} />
          <input placeholder="Search memory" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <label className="inline">As of
          <input className="date-input" value={asOf} onChange={(e) => setAsOf(e.target.value)} placeholder="YYYY-MM-DD" aria-label="As of date" />
        </label>
        {asOf !== todayISO() && <button className="link" onClick={() => setAsOf(todayISO())}>Today</button>}
      </div>
      {hits ? (
        <div className="facts">
          <h3>{hits.length === 1 ? "1 fact" : `${hits.length} facts`} true on {asOf}</h3>
          {hits.map((f) => <FactRow key={f.id} f={f} current onOpen={onOpen} showTopic />)}
        </div>
      ) : (
        <div className="memory-cols">
          <nav className="topics">
            {topics.map(([t, n]) => (
              <button key={t} className={t === topic ? "topic on" : "topic"} onClick={() => setTopic(t)}>
                <span>{t}</span>
                <span className="dim">{n}</span>
              </button>
            ))}
          </nav>
          <div className="facts">
            <h3>{topic}</h3>
            {history.map((f) => <FactRow key={f.id} f={f} current={holds(f)} onOpen={onOpen} onRetract={retract} />)}
          </div>
        </div>
      )}
    </div>
  );
}

function FactRow({ f, current, onOpen, onRetract, showTopic }: { f: Fact; current: boolean; onOpen: (id: string) => void; onRetract?: (f: Fact) => void; showTopic?: boolean }) {
  const span = f.valid_from || f.valid_to ? `${f.valid_from ?? "…"} → ${f.valid_to ?? "now"}` : "since unknown";
  const state = f.retracted_at ? "retracted" : current ? "current" : "past";
  return (
    <div className={`fact ${state}`}>
      <div className="fact-text">
        {showTopic && <span className="badge">{f.topic}</span>}
        {f.text}
      </div>
      <div className="fact-meta">
        <span>{span}</span>
        {state === "retracted" && <span>· retracted {f.retracted_at!.slice(0, 10)}</span>}
        <span>· noted {f.recorded_at.slice(0, 10)}</span>
        {f.source && <button className="link" onClick={() => onOpen(f.source!)}>source entry</button>}
        {onRetract && state !== "retracted" && <button className="link danger-link" onClick={() => onRetract(f)} title="Mark as never true; it stays in history">Wrong</button>}
      </div>
    </div>
  );
}
