import { buildRule, parsePreset } from "./recurrence";

describe("repeat presets", () => {
  it("round-trips presets", () => {
    for (const [preset, days] of [["daily", []], ["weekdays", []], ["weekly", ["MO", "TH"]], ["biweekly", ["SA"]], ["monthly", []]] as const) {
      const rule = buildRule(preset, [...days], "");
      expect(parsePreset(rule).preset).toBe(preset);
    }
    expect(buildRule("none", [], "")).toBeNull();
    expect(parsePreset("FREQ=MONTHLY;BYDAY=-1FR").preset).toBe("custom");
    expect(parsePreset("FREQ=WEEKLY;BYDAY=MO,TH").days).toEqual(["MO", "TH"]);
  });
});
