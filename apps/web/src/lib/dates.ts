/** Date helpers working on local calendar dates (YYYY-MM-DD strings). */

export function pad(n: number): string {
  return String(n).padStart(2, "0");
}

export function isoDate(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function todayIso(now = new Date()): string {
  return isoDate(now);
}

export function parseIsoDate(s: string): Date {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, (m ?? 1) - 1, d ?? 1);
}

export function addDays(iso: string, n: number): string {
  const d = parseIsoDate(iso);
  d.setDate(d.getDate() + n);
  return isoDate(d);
}

export function daysBetween(a: string, b: string): number {
  const ms = parseIsoDate(b).getTime() - parseIsoDate(a).getTime();
  return Math.round(ms / 86_400_000);
}

export function localTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}

const fmtCache = new Map<string, Intl.DateTimeFormat>();
function fmt(opts: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const k = JSON.stringify(opts);
  let f = fmtCache.get(k);
  if (!f) {
    f = new Intl.DateTimeFormat(undefined, opts);
    fmtCache.set(k, f);
  }
  return f;
}

/** "Today", "Tomorrow", "Yesterday", weekday within a week, else "Mon, Oct 12". */
export function friendlyDate(iso: string, today = todayIso()): string {
  const diff = daysBetween(today, iso);
  if (diff === 0) return "Today";
  if (diff === 1) return "Tomorrow";
  if (diff === -1) return "Yesterday";
  const d = parseIsoDate(iso);
  if (diff > 1 && diff < 7) return fmt({ weekday: "long" }).format(d);
  return fmt({ weekday: "short", month: "short", day: "numeric" }).format(d);
}

/** Gentle phrasing for items whose date has passed — no "OVERDUE!" shouting. */
export function gentleDue(iso: string, today = todayIso()): string {
  const diff = daysBetween(today, iso);
  if (diff >= 0) return friendlyDate(iso, today);
  if (diff === -1) return "From yesterday";
  if (diff > -7) return `From ${fmt({ weekday: "long" }).format(parseIsoDate(iso))}`;
  return "From earlier";
}

export function timeOf(date: Date): string {
  return fmt({ hour: "2-digit", minute: "2-digit" }).format(date);
}

export function longDate(iso: string): string {
  return fmt({ weekday: "long", month: "long", day: "numeric" }).format(parseIsoDate(iso));
}

export function monthTitle(year: number, month: number): string {
  return fmt({ month: "long", year: "numeric" }).format(new Date(year, month, 1));
}

export function weekdayShort(i: number): string {
  // 0 = Monday
  return fmt({ weekday: "short" }).format(new Date(2024, 0, 1 + i));
}

export function relativeFrom(target: Date, now = new Date()): string {
  const ms = target.getTime() - now.getTime();
  const abs = Math.abs(ms);
  const mins = Math.round(abs / 60_000);
  const hours = Math.round(abs / 3_600_000);
  const days = Math.round(abs / 86_400_000);
  const s = mins < 60 ? `${mins} min` : hours < 48 ? `${hours} h` : `${days} days`;
  return ms >= 0 ? `in ${s}` : `${s} ago`;
}

export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

/** Human summary used for screen readers ("12 minutes left"). */
export function spokenDuration(ms: number): string {
  const mins = Math.ceil(ms / 60_000);
  if (mins <= 1) return "less than a minute";
  if (mins < 60) return `${mins} minutes`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m ? `${h} hour${h > 1 ? "s" : ""} ${m} minutes` : `${h} hour${h > 1 ? "s" : ""}`;
}

export function countdownLabel(target: Date, now = new Date()): string {
  const ms = target.getTime() - now.getTime();
  if (ms <= 0) return "It's here";
  const days = Math.floor(ms / 86_400_000);
  if (days >= 2) return `${days} days`;
  const hours = Math.floor(ms / 3_600_000);
  if (hours >= 1) return `${hours} h ${Math.floor((ms % 3_600_000) / 60_000)} min`;
  return `${Math.ceil(ms / 60_000)} min`;
}
