import type { TimerView } from "@tendly/contracts";
import { clockOffset, progress, remainingMs } from "./timer";

function view(partial: Partial<TimerView["state"]>, serverNow: string): TimerView {
  return {
    state: {
      kind: "focus",
      status: "running",
      phase: "focus",
      completedFocus: 0,
      durationMs: 600_000,
      endsAt: null,
      remainingMs: null,
      phaseEndedAt: null,
      label: null,
      taskId: null,
      config: { focusMinutes: 25, shortBreakMinutes: 5, longBreakMinutes: 15, cyclesBeforeLongBreak: 4, autoStartBreaks: true, autoStartFocus: false },
      ...partial,
    },
    remainingMs: 0,
    serverNow,
    version: 1,
    phaseJustEnded: false,
  };
}

describe("timer display", () => {
  it("uses the absolute deadline and corrects for device clock skew", () => {
    const v = view({ endsAt: "2026-10-03T12:10:00Z" }, "2026-10-03T12:00:00Z");
    // Device clock runs 2 minutes behind the server.
    const received = Date.parse("2026-10-03T11:58:00Z");
    const off = clockOffset(v, received);
    expect(remainingMs(v, off, received)).toBe(600_000);
    // After "sleeping" for an hour the display shows zero, not a stale value.
    expect(remainingMs(v, off, received + 3_600_000)).toBe(0);
    expect(progress(v, 300_000)).toBeCloseTo(0.5);
  });
  it("shows paused remaining time", () => {
    const v = view({ status: "paused", remainingMs: 123_000 }, "2026-10-03T12:00:00Z");
    expect(remainingMs(v, 0, Date.now())).toBe(123_000);
  });
});
