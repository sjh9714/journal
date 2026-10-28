import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, IN_TAURI } from "../api";
import type { Entry, ImportReport, LockInfo, Problem, ReminderConfig, Status, TrashedEntry } from "../types";
import { captureEnabled, setCaptureEnabled } from "../lib/enrich";

export type Theme = "system" | "light" | "dark";

export function Settings({ status, theme, onTheme, onStatus, onImported, problems, onReindexed, onRestored, onLockChanged, onLockNow }: {
  status: Status;
  theme: Theme;
  onTheme: (t: Theme) => void;
  onStatus: (s: Status) => void;
  onImported: () => void;
  problems: Problem[];
  onReindexed: () => void;
  onRestored: (e: Entry) => void;
  onLockChanged: () => Promise<LockInfo>;
  onLockNow: () => void;
}) {
  const [trash, setTrash] = useState<TrashedEntry[]>([]);
  const [lock, setLock] = useState<LockInfo>({ enabled: false, idle_minutes: 5 });
  const [reminder, setReminder] = useState<ReminderConfig>({ enabled: false, time: "21:00" });
  const [pw, setPw] = useState({ current: "", next: "", again: "" });
  const [lockMsg, setLockMsg] = useState<string | null>(null);

  const loadTrash = useCallback(() => api.trashList().then(setTrash).catch(() => setTrash([])), []);
  useEffect(() => { loadTrash(); api.lockInfo().then(setLock); api.getReminder().then(setReminder); }, [loadTrash]);

  const savePasscode = async () => {
    setLockMsg(null);
    if (pw.next !== pw.again) return setLockMsg("The two new passcodes differ.");
    try {
      setLock(await api.setPasscode(lock.enabled ? pw.current : null, pw.next, lock.idle_minutes));
      setPw({ current: "", next: "", again: "" });
      setLockMsg(lock.enabled ? "Passcode changed." : "Lock turned on.");
      await onLockChanged();
    } catch (e) {
      setLockMsg(String(e));
    }
  };
  const removePasscode = async () => {
    setLockMsg(null);
    try {
      setLock(await api.clearPasscode(pw.current));
      setPw({ current: "", next: "", again: "" });
      setLockMsg("Lock turned off.");
      await onLockChanged();
    } catch (e) {
      setLockMsg(String(e));
    }
  };
  const changeIdle = async (m: number) => {
    setLock(await api.setLockIdle(m));
    await onLockChanged();
  };
  const saveReminder = async (r: ReminderConfig) => setReminder(await api.setReminder(r.enabled, r.time));
  const [capture, setCapture] = useState(captureEnabled());
  const [report, setReport] = useState<ImportReport | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  useEffect(() => setCaptureEnabled(capture), [capture]);

  const changeRoot = async (create: boolean) => {
    const dir = IN_TAURI ? await open({ directory: true, multiple: false }) : "/Users/demo/Other";
    if (typeof dir !== "string") return;
    try {
      onStatus(await api.openRoot(dir, create));
      setMsg(`Now using ${dir}`);
    } catch (e) {
      setMsg(String(e));
    }
  };
  const importDayOne = async () => {
    const file = IN_TAURI ? await open({ multiple: false, directory: false, filters: [{ name: "Day One export", extensions: ["zip", "json"] }] }) : "/tmp/export.zip";
    if (typeof file !== "string") return;
    const path = file.endsWith(".json") ? file.slice(0, file.lastIndexOf("/")) : file;
    setMsg("Importing…");
    try {
      const r = await api.importDayOne(path);
      setReport(r);
      setMsg(null);
      onImported();
      onStatus(await api.status());
    } catch (e) {
      setMsg(String(e));
    }
  };
  const reindex = async () => {
    const s = await api.reindex(true);
    setMsg(`Rebuilt index: ${s.added} entries, ${s.skipped} skipped`);
    onStatus(await api.status());
    onReindexed();
  };
  const reveal = (rel: string) => {
    if (IN_TAURI && status.root) revealItemInDir(`${status.root}/${rel}`);
  };
  const mcpCmd = "claude mcp add journal -- journal mcp serve";

  return (
    <div className="settings">
      <h2>Settings</h2>

      <section>
        <h3>Journal folder</h3>
        <p className="mono">{status.root}</p>
        <p className="dim">{status.entries} entries · index at <span className="mono">{status.index_path}</span></p>
        <div className="btn-row">
          {IN_TAURI && status.root && <button onClick={() => revealItemInDir(status.root!)}>Reveal in Finder</button>}
          <button onClick={() => changeRoot(false)}>Open another journal…</button>
          <button onClick={() => changeRoot(true)}>Create new journal…</button>
          <button onClick={reindex}>Rebuild search index</button>
        </div>
        {problems.length > 0 && (
          <div className="problems">
            <p className="problem-title">{problems.length === 1 ? "1 file could not be read" : `${problems.length} files could not be read`}. They are left untouched and are not searchable until fixed. Sync conflicts usually look like “… (conflicted copy).md”.</p>
            <ul>
              {problems.map((pr) => (
                <li key={pr.path}>
                  <span className="mono">{pr.path}</span>
                  <span className="dim"> · {pr.reason}</span>
                  {IN_TAURI && <button className="link" onClick={() => reveal(pr.path)}>Show file</button>}
                </li>
              ))}
            </ul>
          </div>
        )}
      </section>

      <section>
        <h3>Appearance</h3>
        <div className="btn-row">
          {(["system", "light", "dark"] as Theme[]).map((t) => (
            <button key={t} className={theme === t ? "on" : ""} onClick={() => onTheme(t)}>{t}</button>
          ))}
        </div>
      </section>

      <section>
        <h3>New entries</h3>
        <label className="check">
          <input type="checkbox" checked={capture} onChange={(e) => setCapture(e.target.checked)} />
          Record location and weather when I start an entry
        </label>
        <p className="dim">Uses the system location service when available, otherwise a city-level guess from your network connection (ipapi.co), then Open-Meteo for the weather. No account or key. Nothing else leaves this machine.</p>
      </section>

      <section>
        <h3>Import from Day One</h3>
        <p className="dim">In Day One choose Export → JSON. Pick the .zip here. Entries, photos, tags, location and weather come across; re-importing never duplicates.</p>
        <div className="btn-row"><button onClick={importDayOne}>Import Day One export…</button></div>
        {report && (
          <p className="dim">
            Imported {report.created} of {report.entries} entries into {report.journals.join(", ")} ({report.already_present} already present, {report.attachments} photos, {report.missing_media} missing media).
          </p>
        )}
      </section>

      <section>
        <h3>Agents (MCP)</h3>
        <p className="dim">Install the <span className="mono">journal</span> CLI, then let Claude Code read and write this journal:</p>
        <pre className="mono">{mcpCmd}</pre>
        <p className="dim">Add <span className="mono">--read-only</span> to allow reading only. Point it at this folder with <span className="mono">--root {status.root}</span> if it is not the default.</p>
      </section>

      <section>
        <h3>Lock</h3>
        <p className="dim">Asks for a passcode when the app opens and after it sits idle. It hides the window only; your files stay plain Markdown and the CLI and MCP server are unaffected.</p>
        {lock.enabled && (
          <div className="btn-row">
            <label className="inline">Lock after
              <select value={lock.idle_minutes} onChange={(e) => changeIdle(Number(e.target.value))}>
                <option value={1}>1 minute</option>
                <option value={5}>5 minutes</option>
                <option value={15}>15 minutes</option>
                <option value={0}>only on launch</option>
              </select>
            </label>
            <button onClick={onLockNow}>Lock now</button>
          </div>
        )}
        <div className="pw-grid">
          {lock.enabled && <input type="password" placeholder="Current passcode" value={pw.current} onChange={(e) => setPw({ ...pw, current: e.target.value })} />}
          <input type="password" placeholder={lock.enabled ? "New passcode" : "Passcode (4+ characters)"} value={pw.next} onChange={(e) => setPw({ ...pw, next: e.target.value })} />
          <input type="password" placeholder="Repeat" value={pw.again} onChange={(e) => setPw({ ...pw, again: e.target.value })} />
        </div>
        <div className="btn-row">
          <button onClick={savePasscode} disabled={!pw.next}>{lock.enabled ? "Change passcode" : "Turn on lock"}</button>
          {lock.enabled && <button onClick={removePasscode} disabled={!pw.current}>Turn off lock</button>}
        </div>
        {lockMsg && <p className="dim">{lockMsg}</p>}
      </section>

      <section>
        <h3>Reminder</h3>
        <label className="check">
          <input type="checkbox" checked={reminder.enabled} onChange={(e) => saveReminder({ ...reminder, enabled: e.target.checked })} />
          Remind me at
          <select value={reminder.time.slice(0, 2)} onChange={(e) => saveReminder({ ...reminder, time: `${e.target.value}:${reminder.time.slice(3, 5)}` })} aria-label="Hour">
            {Array.from({ length: 24 }, (_, h) => String(h).padStart(2, "0")).map((h) => <option key={h} value={h}>{h}</option>)}
          </select>
          :
          <select value={reminder.time.slice(3, 5)} onChange={(e) => saveReminder({ ...reminder, time: `${reminder.time.slice(0, 2)}:${e.target.value}` })} aria-label="Minute">
            {Array.from({ length: 12 }, (_, i) => String(i * 5).padStart(2, "0")).map((m) => <option key={m} value={m}>{m}</option>)}
          </select>
          if I have not written today
        </label>
        <p className="dim">Works while the app is open. macOS asks once to allow notifications.</p>
      </section>

      <section>
        <h3>Trash</h3>
        {trash.length === 0 ? (
          <p className="dim">Empty. Deleted entries wait here for 30 days, then are removed.</p>
        ) : (
          <>
            <p className="dim">{trash.length} {trash.length === 1 ? "entry" : "entries"}. Removed for good 30 days after deletion.</p>
            <ul className="trash-list">
              {trash.map((t) => (
                <li key={t.trash_path}>
                  <span className="trash-title">{t.title || <em className="dim">Untitled</em>}</span>
                  <span className="dim"> · written {t.created.slice(0, 10)} · deleted {t.deleted_at.slice(0, 10)}</span>
                  <button className="link" onClick={async () => { const e = await api.restore(t.uuid); await loadTrash(); onRestored(e); }}>Restore</button>
                </li>
              ))}
            </ul>
            <div className="btn-row"><button onClick={async () => { await api.emptyTrash(); loadTrash(); }}>Empty trash</button></div>
          </>
        )}
      </section>

      <section>
        <h3>Help</h3>
        <p className="dim">Found a bug or an export that would not import? No telemetry leaves this app, so a report is the only way we hear about it.</p>
        <div className="btn-row">
          <button onClick={() => (IN_TAURI ? openUrl("https://github.com/sjh9714/journal/issues/new/choose") : window.open("https://github.com/sjh9714/journal/issues/new/choose"))}>Report a problem</button>
          <button onClick={() => (IN_TAURI ? openUrl("https://github.com/sjh9714/journal") : window.open("https://github.com/sjh9714/journal"))}>Source on GitHub</button>
        </div>
      </section>

      {msg && <p className="dim">{msg}</p>}
    </div>
  );
}
