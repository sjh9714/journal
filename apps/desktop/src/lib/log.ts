import { invoke } from "@tauri-apps/api/core";
import { IN_TAURI } from "../api";

export function report(level: "error" | "warn", msg: unknown) {
  const text = msg instanceof Error ? `${msg.message}\n${msg.stack ?? ""}` : String(msg);
  console.error(text);
  if (IN_TAURI) invoke("log_js", { level, msg: text }).catch(() => {});
}

export function installGlobalHandlers() {
  window.addEventListener("error", (e) => report("error", e.error ?? e.message));
  window.addEventListener("unhandledrejection", (e) => report("error", e.reason));
}
