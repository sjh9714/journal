import { useEffect, useRef, useState } from "react";
import { Lock } from "lucide-react";
import { api } from "../api";

export function LockScreen({ onUnlock }: { onUnlock: () => void }) {
  const [pass, setPass] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [wait, setWait] = useState(0);
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => { ref.current?.focus(); }, []);
  useEffect(() => {
    if (wait <= 0) return;
    const t = window.setTimeout(() => setWait((w) => w - 1), 1000);
    return () => window.clearTimeout(t);
  }, [wait]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!pass || wait > 0) return;
    const r = await api.verifyPasscode(pass);
    if (r.ok) {
      setPass("");
      onUnlock();
      return;
    }
    setPass("");
    if (r.retry_after > 0) {
      setWait(r.retry_after);
      setError(`Too many attempts. Try again in ${r.retry_after}s.`);
    } else {
      setError("Wrong passcode.");
    }
    ref.current?.focus();
  };

  return (
    <div className="lock" data-tauri-drag-region>
      <form className="lock-card" onSubmit={submit}>
        <Lock size={22} />
        <h2>Journal is locked</h2>
        <input
          ref={ref}
          type="password"
          autoComplete="current-password"
          placeholder="Passcode"
          value={pass}
          disabled={wait > 0}
          onChange={(e) => { setPass(e.target.value); setError(null); }}
        />
        <button className="primary" type="submit" disabled={wait > 0 || !pass}>{wait > 0 ? `Wait ${wait}s` : "Unlock"}</button>
        {error && <p className="error">{error}</p>}
        <p className="dim small">Forgot it? Quit the app, open its config.json (Settings shows where) and delete the <span className="mono">lock</span> entry. Your entries are plain files and are never encrypted by this lock.</p>
      </form>
    </div>
  );
}
