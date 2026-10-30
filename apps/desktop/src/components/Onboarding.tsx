import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, IN_TAURI } from "../api";
import type { Status } from "../types";

export function Onboarding({ status, onReady }: { status: Status; onReady: (s: Status) => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const go = async (path: string, create: boolean) => {
    setBusy(true);
    setError(null);
    try {
      onReady(await api.openRoot(path, create));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const pick = async (create: boolean) => {
    if (!IN_TAURI) return go("/Users/demo/Journal", create);
    const dir = await open({ directory: true, multiple: false, title: create ? "Choose where to create your journal" : "Choose an existing journal folder" });
    if (typeof dir === "string") go(dir, create);
  };

  return (
    <div className="onboard" data-tauri-drag-region>
      <div className="onboard-card">
        <h1>Dayfile</h1>
        <p>Your entries are plain Markdown files in a folder you own. Sync them with any tool, read them with any editor, or let your agent in over MCP.</p>
        <button className="primary" disabled={busy} onClick={() => go(status.default_root, true)}>
          Create at {status.default_root}
        </button>
        <div className="onboard-row">
          <button disabled={busy} onClick={() => pick(true)}>Create in another folder…</button>
          <button disabled={busy} onClick={() => pick(false)}>Open existing journal…</button>
        </div>
        {error && <p className="error">{error}</p>}
      </div>
    </div>
  );
}
