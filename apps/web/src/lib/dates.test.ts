import { addDays, countdownLabel, daysBetween, formatDuration, gentleDue, spokenDuration } from "./dates";

describe("dates", () => {
  it("adds days across month and DST boundaries", () => {
    expect(addDays("2026-03-28", 3)).toBe("2026-03-31");
    expect(addDays("2026-10-24", 2)).toBe("2026-10-26");
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
    expect(daysBetween("2026-10-24", "2026-11-02")).toBe(9);
  });
  it("phrases past items gently", () => {
    expect(gentleDue("2026-10-02", "2026-10-03")).toBe("From yesterday");
    expect(gentleDue("2026-09-01", "2026-10-03")).toBe("From earlier");
    expect(gentleDue("2026-10-03", "2026-10-03")).toBe("Today");
    expect(gentleDue("2026-10-04", "2026-10-03")).toBe("Tomorrow");
  });
  it("formats durations for eyes and ears", () => {
    expect(formatDuration(25 * 60_000)).toBe("25:00");
    expect(formatDuration(3_725_000)).toBe("1:02:05");
    expect(formatDuration(-5)).toBe("00:00");
    expect(spokenDuration(12 * 60_000)).toBe("12 minutes");
    expect(spokenDuration(30_000)).toBe("less than a minute");
    expect(spokenDuration(90 * 60_000)).toBe("1 hour 30 minutes");
  });
  it("counts down to events", () => {
    const now = new Date("2026-10-03T12:00:00Z");
    expect(countdownLabel(new Date("2026-10-13T12:00:00Z"), now)).toBe("10 days");
    expect(countdownLabel(new Date("2026-10-03T11:00:00Z"), now)).toBe("It's here");
  });
});
