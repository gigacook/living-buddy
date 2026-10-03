/** Small helpers to build the RRULE strings the server understands. */
export type RepeatPreset = "none" | "daily" | "weekdays" | "weekly" | "biweekly" | "monthly" | "custom";

export const WEEKDAY_CODES = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"] as const;
export const WEEKDAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function buildRule(preset: RepeatPreset, days: string[], custom: string): string | null {
  switch (preset) {
    case "none":
      return null;
    case "daily":
      return "FREQ=DAILY";
    case "weekdays":
      return "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR";
    case "weekly":
      return days.length ? `FREQ=WEEKLY;BYDAY=${days.join(",")}` : "FREQ=WEEKLY";
    case "biweekly":
      return days.length ? `FREQ=WEEKLY;INTERVAL=2;BYDAY=${days.join(",")}` : "FREQ=WEEKLY;INTERVAL=2";
    case "monthly":
      return "FREQ=MONTHLY";
    case "custom":
      return custom.trim() || null;
  }
}

export function parsePreset(rule: string | null | undefined): { preset: RepeatPreset; days: string[] } {
  if (!rule) return { preset: "none", days: [] };
  const parts = Object.fromEntries(rule.split(";").map((p) => p.split("=") as [string, string]));
  const days = parts.BYDAY ? parts.BYDAY.split(",").filter((d: string) => (WEEKDAY_CODES as readonly string[]).includes(d)) : [];
  const keys = Object.keys(parts).sort().join(",");
  if (rule === "FREQ=DAILY") return { preset: "daily", days: [] };
  if (rule === "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR") return { preset: "weekdays", days };
  if (parts.FREQ === "WEEKLY" && !parts.INTERVAL && (keys === "FREQ" || keys === "BYDAY,FREQ")) return { preset: "weekly", days };
  if (parts.FREQ === "WEEKLY" && parts.INTERVAL === "2" && (keys === "FREQ,INTERVAL" || keys === "BYDAY,FREQ,INTERVAL")) return { preset: "biweekly", days };
  if (rule === "FREQ=MONTHLY") return { preset: "monthly", days: [] };
  return { preset: "custom", days: [] };
}
