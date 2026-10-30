import { convertFileSrc } from "@tauri-apps/api/core";
import { IN_TAURI } from "../api";

/** Base URL the `dayfile://` scheme answers on, e.g. dayfile://localhost/ */
const BASE = IN_TAURI ? convertFileSrc("", "dayfile") : "/__dayfile__/";
const DISK_PREFIX = "../../attachments/";
const WEB_PREFIX = `${BASE}attachments/`;

/** Rewrite relative attachment links so the webview can load them. */
export function toEditor(md: string): string {
  return md.split(DISK_PREFIX).join(WEB_PREFIX);
}

/** Reverse of `toEditor`; what gets written to the .md file. */
export function toDisk(md: string): string {
  const linked = md.split(WEB_PREFIX).join(DISK_PREFIX).split(encodeURI(WEB_PREFIX)).join(DISK_PREFIX);
  // Tiptap writes empty paragraphs as `&nbsp;`; Markdown has no empty paragraph, so drop them.
  return linked
    .split("\n")
    .map((l) => (l.trim() === "&nbsp;" ? "" : l))
    .join("\n")
    .replace(/\n{3,}/g, "\n\n");
}

export function attachmentUrl(rel: string): string {
  return `${BASE}${rel}`;
}
