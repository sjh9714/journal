export interface Location {
  latitude: number;
  longitude: number;
  place_name?: string | null;
}
export interface Weather {
  temperature_c?: number | null;
  conditions?: string | null;
}
export interface EntrySummary {
  uuid: string;
  created: string;
  date: string;
  journal: string;
  tags: string[];
  starred: boolean;
  title: string;
  path: string;
}
export interface Entry {
  uuid: string;
  created: string;
  timezone: string;
  journal: string;
  tags: string[];
  starred: boolean;
  location?: Location | null;
  weather?: Weather | null;
  extra?: unknown;
  body: string;
  path: string;
}
export interface Status {
  root: string | null;
  index_path: string | null;
  entries: number;
  default_root: string;
}
export interface Filter {
  journal?: string;
  tag?: string;
  from?: string;
  to?: string;
  starred?: boolean;
  limit?: number;
  offset?: number;
  ascending?: boolean;
}
export interface NewEntry {
  body: string;
  created?: string;
  timezone?: string;
  journal?: string;
  tags?: string[];
  starred?: boolean;
  location?: Location | null;
  weather?: Weather | null;
}
export interface EntryPatch {
  body?: string;
  tags?: string[];
  starred?: boolean;
  journal?: string;
  location?: Location | null;
  weather?: Weather | null;
}
export interface ImportReport {
  files: string[];
  entries: number;
  created: number;
  already_present: number;
  attachments: number;
  missing_media: number;
  journals: string[];
}
export interface ReindexStats {
  added: number;
  updated: number;
  removed: number;
  unchanged: number;
  skipped: number;
}
export interface Problem {
  path: string;
  reason: string;
}
