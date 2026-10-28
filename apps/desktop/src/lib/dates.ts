import { format, parseISO } from "date-fns";

export function todayISO(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function monthKey(date: string): string {
  return date.slice(0, 7);
}

export function monthLabel(key: string): string {
  return format(parseISO(`${key}-01`), "MMMM yyyy");
}

export function dayLabel(date: string): string {
  return format(parseISO(date), "EEEE, MMMM d, yyyy");
}

export function timeLabel(created: string): string {
  // Keep the entry's own offset rather than converting to the machine zone.
  const m = created.match(/T(\d{2}):(\d{2})/);
  return m ? `${m[1]}:${m[2]}` : "";
}

export function yearsAgo(date: string): string {
  const y = new Date().getFullYear() - Number(date.slice(0, 4));
  return y === 1 ? "1 year ago" : `${y} years ago`;
}

export function localRfc3339(d = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  const off = -d.getTimezoneOffset();
  const sign = off >= 0 ? "+" : "-";
  const a = Math.abs(off);
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}` +
    `${sign}${pad(Math.floor(a / 60))}:${pad(a % 60)}`
  );
}
