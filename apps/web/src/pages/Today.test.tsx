import type { Task } from "@tendly/contracts";
import { pickNext } from "./Today";

const base: Task = {
  id: "t",
  groupId: null,
  title: "x",
  notes: null,
  category: "home",
  priority: "normal",
  durationMinutes: null,
  ownerId: "me",
  assigneeId: "me",
  dueDate: null,
  dueTime: null,
  startDate: null,
  startTime: null,
  deadline: null,
  timezone: "UTC",
  recurrence: null,
  recurrenceLabel: null,
  repeatMode: "fixed",
  rotation: [],
  columnKey: null,
  milestoneId: null,
  tags: [],
  subtasks: [],
  position: 0,
  completedAt: null,
  completedBy: null,
  completionCount: 0,
  templateKey: null,
  version: 1,
  createdAt: "",
  updatedAt: "",
};
const t = (p: Partial<Task>): Task => ({ ...base, ...p });

describe("pickNext", () => {
  const today = "2026-10-03";
  it("prefers today's items, then importance, then time", () => {
    const tasks = [
      t({ id: "old", dueDate: "2026-09-01", priority: "high" }),
      t({ id: "later", dueDate: today, dueTime: "18:00" }),
      t({ id: "soon", dueDate: today, dueTime: "09:00" }),
      t({ id: "important", dueDate: today, priority: "high", dueTime: "20:00" }),
    ];
    expect(pickNext(tasks, "me", today)?.id).toBe("important");
    expect(pickNext(tasks.filter((x) => x.id !== "important"), "me", today)?.id).toBe("soon");
  });
  it("ignores other people's and future items", () => {
    const tasks = [t({ id: "theirs", dueDate: today, assigneeId: "sam" }), t({ id: "future", dueDate: "2026-10-09" })];
    expect(pickNext(tasks, "me", today)).toBeUndefined();
    expect(pickNext([...tasks, t({ id: "leftover", dueDate: "2026-10-01" })], "me", today)?.id).toBe("leftover");
  });
});
