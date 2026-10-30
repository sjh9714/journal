import { api } from "../api";
import type { CaptureReport } from "../types";
import { report } from "./log";

/** Capture today when the user has set up at least one source. */
export async function captureIfConfigured(): Promise<CaptureReport | null> {
  try {
    const cfg = await api.getCapture();
    if (!cfg.git.length && !cfg.calendars.length && !cfg.photos.length) return null;
    return await api.captureDay();
  } catch (e) {
    report("warn", `capture failed: ${e}`);
    return null;
  }
}

export function describe(r: CaptureReport) {
  const n = (k: number, one: string, many: string) => (k === 1 ? `1 ${one}` : `${k} ${many}`);
  return [r.commits && n(r.commits, "commit", "commits"), r.events && n(r.events, "event", "events"), r.photos && n(r.photos, "photo", "photos")].filter(Boolean).join(", ");
}
