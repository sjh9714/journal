import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
import { api, IN_TAURI } from "../api";
import type { CaptureConfig, CaptureReport } from "../types";

/** Settings → Capture: where a day's draft comes from. */
export function CaptureSettings({ onCaptured }: { onCaptured: (r: CaptureReport) => void }) {
  const [cfg, setCfg] = useState<CaptureConfig | null>(null);
  const [calendar, setCalendar] = useState("");
  const [author, setAuthor] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => { api.getCapture().then((c) => { setCfg(c); setAuthor(c.git_author ?? ""); }); }, []);
  if (!cfg) return null;

  const save = async (next: CaptureConfig) => setCfg(await api.setCapture(next));
  const pickFolder = async () => {
    const dir = IN_TAURI ? await open({ directory: true, multiple: false }) : "~/Pictures/Phone";
    return typeof dir === "string" ? dir : null;
  };
  const addGit = async () => { const d = await pickFolder(); if (d && !cfg.git.includes(d)) save({ ...cfg, git: [...cfg.git, d] }); };
  const addPhotos = async () => { const d = await pickFolder(); if (d && !cfg.photos.includes(d)) save({ ...cfg, photos: [...cfg.photos, d] }); };
  const addCalendar = () => {
    const c = calendar.trim();
    if (c && !cfg.calendars.includes(c)) save({ ...cfg, calendars: [...cfg.calendars, c] });
    setCalendar("");
  };
  const captureNow = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const r = await api.captureDay();
      setMsg(r.added === 0 ? "Nothing new today." : `Added ${r.commits} commits, ${r.events} events, ${r.photos} photos to today's draft.`);
      if (r.errors.length) setMsg((m) => `${m ?? ""} ${r.errors.join(" · ")}`);
      onCaptured(r);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };
  const remove = (key: "git" | "calendars" | "photos", v: string) => save({ ...cfg, [key]: cfg[key].filter((x) => x !== v) });

  return (
    <section>
      <h3>Capture</h3>
      <p className="dim">Dayfile drafts your day from these sources when the app opens, at the reminder time, and when you press Capture. Nothing runs in the background, and only the sources listed here are read. Drafts wait for you to keep or discard them.</p>

      <h4>Git folders</h4>
      <p className="dim">Repositories in these folders, a few levels deep. Only your own commits.</p>
      <SourceList items={cfg.git} onRemove={(v) => remove("git", v)} />
      <div className="btn-row">
        <button onClick={addGit}>Add folder…</button>
        <input className="grow-input" placeholder="Your commit email" value={author} onChange={(e) => setAuthor(e.target.value)} onBlur={() => save({ ...cfg, git_author: author.trim() || null })} />
      </div>

      <h4>Calendars</h4>
      <p className="dim">An .ics file or a calendar feed URL. In Google Calendar: Settings → your calendar → Secret address in iCal format. In Apple Calendar: share the calendar publicly and copy the link.</p>
      <SourceList items={cfg.calendars} onRemove={(v) => remove("calendars", v)} />
      <div className="btn-row">
        <input className="grow-input" placeholder="https://… or ~/calendar.ics" value={calendar} onChange={(e) => setCalendar(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addCalendar()} />
        <button onClick={addCalendar} disabled={!calendar.trim()}>Add</button>
      </div>

      <h4>Photo folders</h4>
      <p className="dim">Photos whose camera time falls on the day are copied in, up to 12 a day.</p>
      <SourceList items={cfg.photos} onRemove={(v) => remove("photos", v)} />
      <div className="btn-row"><button onClick={addPhotos}>Add folder…</button></div>

      <div className="btn-row">
        <button className="primary" onClick={captureNow} disabled={busy || (!cfg.git.length && !cfg.calendars.length && !cfg.photos.length)}>{busy ? "Capturing…" : "Capture today"}</button>
      </div>
      {msg && <p className="dim">{msg}</p>}
    </section>
  );
}

function SourceList({ items, onRemove }: { items: string[]; onRemove: (v: string) => void }) {
  if (items.length === 0) return null;
  return (
    <ul className="source-list">
      {items.map((v) => (
        <li key={v}>
          <span className="mono">{v}</span>
          <button className="icon-btn tiny" title="Remove" onClick={() => onRemove(v)}><X size={13} /></button>
        </li>
      ))}
    </ul>
  );
}
