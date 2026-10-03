import type { TimerView } from "@tendly/contracts";

/**
 * Computes remaining time for display from the server's absolute deadline.
 * `offsetMs` corrects for the difference between server and device clocks,
 * measured when the timer was fetched. The server stays the source of truth;
 * this only drives the on-screen countdown between fetches.
 */
export function clockOffset(view: TimerView, receivedAt = Date.now()): number {
  return new Date(view.serverNow).getTime() - receivedAt;
}

export function remainingMs(view: TimerView, offsetMs: number, now = Date.now()): number {
  const s = view.state;
  if (s.status === "running" && s.endsAt) return Math.max(0, new Date(s.endsAt).getTime() - (now + offsetMs));
  if (s.status === "paused") return s.remainingMs ?? 0;
  if (s.status === "idle") return s.durationMs;
  return 0;
}

export function progress(view: TimerView, remaining: number): number {
  const total = view.state.durationMs || 1;
  return Math.min(1, Math.max(0, 1 - remaining / total));
}

export function phaseLabel(view: TimerView): string {
  const s = view.state;
  if (s.kind === "countdown") return "Countdown";
  if (s.kind === "focus") return "Focus";
  return s.phase === "focus" ? "Focus" : s.phase === "short_break" ? "Short break" : "Long break";
}
