import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, IN_TAURI } from "../api";
import type { ImportReport, Status } from "../types";
import { captureEnabled, setCaptureEnabled } from "../lib/enrich";

export type Theme = "system" | "light" | "dark";

export function Settings({ status, theme, onTheme, onStatus, onImported }: {
  status: Status;
  theme: Theme;
  onTheme: (t: Theme) => void;
  onStatus: (s: Status) => void;
  onImported: () => void;
}) {
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
        <p className="dim">Uses the system location service and Open-Meteo (no account, no key). Nothing else leaves this machine.</p>
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

      {msg && <p className="dim">{msg}</p>}
    </div>
  );
}
