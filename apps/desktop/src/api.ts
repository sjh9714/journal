import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { Entry, EntryPatch, EntrySummary, Filter, ImportReport, NewEntry, Problem, ReindexStats, Status } from "./types";

export const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (IN_TAURI) return tauriInvoke<T>(cmd, args);
  const { mockInvoke } = await import("./mock");
  return mockInvoke(cmd, args) as Promise<T>;
}

/** Rust omits empty/false/null fields when serializing; give the UI stable shapes. */
function normEntry(e: Entry): Entry {
  return { ...e, tags: e.tags ?? [], starred: e.starred ?? false, location: e.location ?? null, weather: e.weather ?? null };
}
function normSummary(s: EntrySummary): EntrySummary {
  return { ...s, tags: s.tags ?? [], starred: s.starred ?? false };
}
const entry = (p: Promise<Entry>) => p.then(normEntry);
const list = (p: Promise<EntrySummary[]>) => p.then((rows) => rows.map(normSummary));

export const api = {
  status: () => invoke<Status>("get_status"),
  openRoot: (path: string, create: boolean) => invoke<Status>("open_root", { path, create }),
  list: (filter: Filter = {}) => list(invoke<EntrySummary[]>("list_entries", { filter })),
  get: (id: string) => entry(invoke<Entry>("get_entry", { id })),
  create: (e: NewEntry) => entry(invoke<Entry>("create_entry", { entry: e })),
  update: (id: string, patch: EntryPatch) => entry(invoke<Entry>("update_entry", { id, patch })),
  remove: (id: string) => entry(invoke<Entry>("delete_entry", { id })),
  search: (query: string, limit = 50) => list(invoke<EntrySummary[]>("search_entries", { query, limit })),
  onThisDay: (date: string) => list(invoke<EntrySummary[]>("on_this_day", { date })),
  onDate: (date: string) => list(invoke<EntrySummary[]>("on_date", { date })),
  tags: () => invoke<[string, number][]>("list_tags"),
  journals: () => invoke<[string, number][]>("list_journals"),
  reindex: (full: boolean) => invoke<ReindexStats>("reindex", { full }),
  problems: () => invoke<Problem[]>("list_problems"),
  attachFile: (path: string) => invoke<{ rel: string; link: string }>("attach_file", { path }),
  importDayOne: (path: string, journal?: string) => invoke<ImportReport>("import_dayone_export", { path, journal: journal ?? null }),
  entryAbsPath: (id: string) => invoke<string>("entry_abs_path", { id }),
};
