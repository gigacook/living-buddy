import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { BellRing, Plus, Trash2 } from "lucide-react";
import type { Category, Priority, RepeatMode, Subtask, Task, TaskInput } from "@tendly/contracts";
import { api, ApiError, errorMessage } from "../lib/api";
import { buildRule, parsePreset, type RepeatPreset } from "../lib/recurrence";
import { localTimezone } from "../lib/dates";
import { TASK_RELATED, useGroups, useInvalidate, useMe, useMembers } from "../lib/queries";
import { CategoryPicker, GroupSelect, MemberSelect, RepeatPicker } from "./pickers";
import { Dialog, Field, Segmented, useToast } from "./ui";

export type TaskDraftDefaults = Partial<Pick<Task, "groupId" | "columnKey" | "dueDate" | "category" | "assigneeId" | "milestoneId">> & { title?: string };

type Form = {
  title: string;
  notes: string;
  category: Category;
  priority: Priority;
  durationMinutes: string;
  groupId: string | null;
  assigneeId: string | null;
  dueDate: string;
  dueTime: string;
  startDate: string;
  deadline: string;
  repeat: { preset: RepeatPreset; days: string[]; custom: string };
  repeatMode: RepeatMode;
  rotation: string[];
  tags: string;
  subtasks: Subtask[];
  columnKey: string | null;
  milestoneId: string | null;
};

function toForm(t: Task | undefined, d: TaskDraftDefaults): Form {
  const r = parsePreset(t?.recurrence);
  return {
    title: t?.title ?? d.title ?? "",
    notes: t?.notes ?? "",
    category: t?.category ?? d.category ?? "personal",
    priority: t?.priority ?? "normal",
    durationMinutes: t?.durationMinutes ? String(t.durationMinutes) : "",
    groupId: t ? t.groupId : (d.groupId ?? null),
    assigneeId: t ? t.assigneeId : (d.assigneeId ?? null),
    dueDate: t?.dueDate ?? d.dueDate ?? "",
    dueTime: t?.dueTime ?? "",
    startDate: t?.startDate ?? "",
    deadline: t?.deadline ?? "",
    repeat: { preset: r.preset, days: r.days, custom: r.preset === "custom" ? (t?.recurrence ?? "") : "" },
    repeatMode: t?.repeatMode ?? "fixed",
    rotation: t?.rotation ?? [],
    tags: (t?.tags ?? []).join(", "),
    subtasks: t?.subtasks ?? [],
    columnKey: t?.columnKey ?? d.columnKey ?? null,
    milestoneId: t?.milestoneId ?? d.milestoneId ?? null,
  };
}

function toChanges(f: Form): Partial<Task> & { subtasks: Subtask[] } {
  return {
    title: f.title,
    notes: f.notes || null,
    category: f.category,
    priority: f.priority,
    durationMinutes: f.durationMinutes ? Number(f.durationMinutes) : null,
    groupId: f.groupId,
    assigneeId: f.assigneeId,
    dueDate: f.dueDate || null,
    dueTime: f.dueTime || null,
    startDate: f.startDate || null,
    deadline: f.deadline || null,
    recurrence: buildRule(f.repeat.preset, f.repeat.days, f.repeat.custom),
    repeatMode: f.repeatMode,
    rotation: f.groupId ? f.rotation : [],
    tags: f.tags
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean),
    subtasks: f.subtasks,
    columnKey: f.columnKey,
    milestoneId: f.milestoneId,
  };
}

export function TaskEditor({ task, defaults = {}, open, onClose }: { task?: Task; defaults?: TaskDraftDefaults; open: boolean; onClose: () => void }) {
  const [form, setForm] = useState<Form>(() => toForm(task, defaults));
  const [tab, setTab] = useState<"details" | "history">("details");
  const [error, setError] = useState<ApiError | null>(null);
  const [conflict, setConflict] = useState<Task | null>(null);
  const [newStep, setNewStep] = useState("");
  const { data: groups = [] } = useGroups();
  const { data: members = [] } = useMembers();
  const me = useMe();
  const invalidate = useInvalidate();
  const toast = useToast();

  useEffect(() => {
    if (open) {
      setForm(toForm(task, defaults));
      setError(null);
      setConflict(null);
      setTab("details");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, task?.id]);

  const group = groups.find((g) => g.id === form.groupId);
  const groupMembers = useMemo(() => (group ? members.filter((m) => group.memberIds.includes(m.id)) : members.filter((m) => m.id === me?.id)), [group, members, me]);
  const set = <K extends keyof Form>(k: K, v: Form[K]) => setForm((f) => ({ ...f, [k]: v }));

  const save = useMutation({
    mutationFn: async (opts: { force?: number } = {}) => {
      const changes = toChanges(form);
      if (task) return api.updateTask(task.id, opts.force ?? conflict?.version ?? task.version, changes);
      const input: TaskInput = {
        ...changes,
        title: form.title,
        timezone: localTimezone(),
        subtasks: form.subtasks.map((s) => s.title),
        notes: changes.notes ?? undefined,
        durationMinutes: changes.durationMinutes ?? undefined,
        groupId: changes.groupId ?? undefined,
        assigneeId: changes.assigneeId ?? undefined,
        dueDate: changes.dueDate ?? undefined,
        dueTime: changes.dueTime ?? undefined,
        startDate: changes.startDate ?? undefined,
        deadline: changes.deadline ?? undefined,
        recurrence: changes.recurrence ?? undefined,
        columnKey: changes.columnKey ?? undefined,
        milestoneId: changes.milestoneId ?? undefined,
      } as TaskInput;
      return api.createTask(input);
    },
    onSuccess: (t) => {
      invalidate(...TASK_RELATED);
      toast.show(task ? `Saved “${t.title}”` : `Added “${t.title}”`);
      onClose();
    },
    onError: (e) => {
      if (e instanceof ApiError && e.status === 409 && e.current) {
        setConflict(e.current as Task);
        setError(null);
      } else {
        setError(e instanceof ApiError ? e : new ApiError(0, "error", errorMessage(e)));
      }
    },
  });

  const remove = useMutation({
    mutationFn: () => api.deleteTask(task!.id),
    onSuccess: () => {
      invalidate(...TASK_RELATED);
      toast.show(`Removed “${task!.title}”`);
      onClose();
    },
    onError: (e) => toast.show(errorMessage(e)),
  });

  const nudge = useMutation({
    mutationFn: () => api.nudge({ toMemberId: task!.assigneeId!, taskId: task!.id }),
    onSuccess: (r) => toast.show(r.deferred ? "Reminder will arrive after their quiet hours." : "Friendly reminder sent."),
    onError: (e) => toast.show(errorMessage(e)),
  });

  const fieldError = (name: string) => (error?.field === name ? error.message : null);
  const isProject = group?.mode === "project";

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={task ? "Edit task" : "New task"}
      footer={
        tab === "details" ? (
          <>
            {task && (
              <button type="button" className="btn btn-ghost btn-danger" onClick={() => remove.mutate()} disabled={remove.isPending}>
                <Trash2 size={18} aria-hidden /> Remove
              </button>
            )}
            <span className="spacer" />
            <button type="button" className="btn" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" form="task-form" className="btn btn-primary" disabled={save.isPending}>
              {save.isPending ? "Saving…" : task ? "Save" : "Add task"}
            </button>
          </>
        ) : undefined
      }
    >
      {task && (
        <div style={{ marginBottom: 16 }}>
          <Segmented
            label="Task sections"
            value={tab}
            onChange={setTab}
            options={[
              { value: "details", label: "Details" },
              { value: "history", label: "History" },
            ]}
          />
        </div>
      )}
      {tab === "history" && task ? (
        <TaskHistoryView taskId={task.id} />
      ) : (
        <form
          id="task-form"
          className="stack"
          onSubmit={(e) => {
            e.preventDefault();
            save.mutate({});
          }}
          noValidate
        >
          {conflict && (
            <div className="banner banner-warn" role="alert">
              <div className="stack-sm" style={{ flex: 1 }}>
                <strong>Someone else changed this task while you were editing.</strong>
                <span>
                  Their version: “{conflict.title}”{conflict.dueDate ? `, due ${conflict.dueDate}` : ""}. You can load theirs or save yours over it.
                </span>
                <div className="row">
                  <button type="button" className="btn btn-sm" onClick={() => { setForm(toForm(conflict, defaults)); setConflict(null); }}>
                    Load their version
                  </button>
                  <button type="button" className="btn btn-sm btn-primary" onClick={() => save.mutate({ force: conflict.version })}>
                    Keep my changes
                  </button>
                </div>
              </div>
            </div>
          )}
          {error && !error.field && (
            <div className="banner banner-danger" role="alert">
              {error.message}
            </div>
          )}
          <Field label="What needs doing?" error={fieldError("title")}>
            {({ id, describedBy, invalid }) => (
              <input id={id} className="input" value={form.title} onChange={(e) => set("title", e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} autoFocus required maxLength={200} />
            )}
          </Field>
          <CategoryPicker value={form.category} onChange={(c) => c && set("category", c)} />
          <div className="form-grid">
            <Field label="Who is it for?">
              {({ id }) => (
                <GroupSelect
                  id={id}
                  groups={groups.filter((g) => !g.archived && me && g.memberIds.includes(me.id))}
                  value={form.groupId}
                  onChange={(g) => setForm((f) => ({ ...f, groupId: g, assigneeId: g ? null : (me?.id ?? null), rotation: [], columnKey: null }))}
                />
              )}
            </Field>
            <Field label="Responsible" error={fieldError("assigneeId")}>
              {({ id, describedBy }) => <MemberSelect id={id} members={groupMembers} value={form.assigneeId} onChange={(v) => set("assigneeId", v)} describedBy={describedBy} />}
            </Field>
          </div>
          <div className="form-grid">
            <Field label="Due date" error={fieldError("dueDate")}>
              {({ id, describedBy, invalid }) => <input id={id} type="date" className="input" value={form.dueDate} onChange={(e) => set("dueDate", e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} />}
            </Field>
            <Field label="Time (optional)" error={fieldError("dueTime")}>
              {({ id, describedBy, invalid }) => <input id={id} type="time" className="input" value={form.dueTime} onChange={(e) => set("dueTime", e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} />}
            </Field>
            <Field label="How long? (minutes)" error={fieldError("durationMinutes")}>
              {({ id, describedBy, invalid }) => (
                <input id={id} type="number" min={1} max={1440} inputMode="numeric" className="input" value={form.durationMinutes} onChange={(e) => set("durationMinutes", e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} />
              )}
            </Field>
          </div>
          <RepeatPicker {...form.repeat} onChange={(r) => set("repeat", r)} />
          {fieldError("recurrence") && (
            <span className="error" role="alert" style={{ color: "var(--danger)", fontWeight: 600 }}>
              {fieldError("recurrence")}
            </span>
          )}
          {form.repeat.preset !== "none" && (
            <>
              <fieldset>
                <legend>When it repeats</legend>
                <label className="check">
                  <input type="radio" name="repeat-mode" checked={form.repeatMode === "fixed"} onChange={() => set("repeatMode", "fixed")} />
                  On a fixed schedule (like bin day)
                </label>
                <label className="check">
                  <input type="radio" name="repeat-mode" checked={form.repeatMode === "after_completion"} onChange={() => set("repeatMode", "after_completion")} />
                  Counting from when it was last done
                </label>
              </fieldset>
              {group && (
                <fieldset>
                  <legend>Take turns (optional)</legend>
                  <p className="small muted" style={{ margin: "4px 0 6px" }}>
                    After each time it's done, it moves to the next person in this order.
                  </p>
                  <div className="chips">
                    {groupMembers.map((m) => (
                      <button
                        key={m.id}
                        type="button"
                        className="chip"
                        aria-pressed={form.rotation.includes(m.id)}
                        onClick={() => set("rotation", form.rotation.includes(m.id) ? form.rotation.filter((x) => x !== m.id) : [...form.rotation, m.id])}
                      >
                        {form.rotation.includes(m.id) ? `${form.rotation.indexOf(m.id) + 1}. ` : ""}
                        {m.displayName}
                      </button>
                    ))}
                  </div>
                </fieldset>
              )}
            </>
          )}
          <fieldset>
            <legend>Checklist</legend>
            <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: "6px 0" }}>
              {form.subtasks.map((s, i) => (
                <li key={s.id || i} className="row">
                  <label className="check" style={{ flex: 1 }}>
                    <input type="checkbox" checked={s.done} onChange={() => set("subtasks", form.subtasks.map((x, j) => (j === i ? { ...x, done: !x.done } : x)))} />
                    {s.title}
                  </label>
                  <button type="button" className="btn btn-ghost btn-icon" aria-label={`Remove step “${s.title}”`} onClick={() => set("subtasks", form.subtasks.filter((_, j) => j !== i))}>
                    <Trash2 size={16} aria-hidden />
                  </button>
                </li>
              ))}
            </ul>
            <div className="row">
              <label htmlFor="new-step" className="visually-hidden">
                New step
              </label>
              <input
                id="new-step"
                className="input"
                style={{ flex: 1 }}
                placeholder="Add a small step"
                value={newStep}
                onChange={(e) => setNewStep(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    if (newStep.trim()) {
                      set("subtasks", [...form.subtasks, { id: "", title: newStep.trim(), done: false }]);
                      setNewStep("");
                    }
                  }
                }}
              />
              <button
                type="button"
                className="btn"
                onClick={() => {
                  if (newStep.trim()) {
                    set("subtasks", [...form.subtasks, { id: "", title: newStep.trim(), done: false }]);
                    setNewStep("");
                  }
                }}
              >
                <Plus size={18} aria-hidden /> Add step
              </button>
            </div>
          </fieldset>
          <details className="disclosure">
            <summary>More options</summary>
            <div className="stack" style={{ marginTop: 8 }}>
              <Field label="Notes">
                {({ id }) => <textarea id={id} className="textarea" value={form.notes} onChange={(e) => set("notes", e.target.value)} maxLength={5000} />}
              </Field>
              <div className="form-grid">
                <Field label="Priority">
                  {({ id }) => (
                    <select id={id} className="select" value={form.priority} onChange={(e) => set("priority", e.target.value as Priority)}>
                      <option value="low">Low</option>
                      <option value="normal">Normal</option>
                      <option value="high">Important</option>
                    </select>
                  )}
                </Field>
                <Field label="Start date" error={fieldError("startDate")}>
                  {({ id }) => <input id={id} type="date" className="input" value={form.startDate} onChange={(e) => set("startDate", e.target.value)} />}
                </Field>
                <Field label="Hard deadline" hint="For things with a real cut-off." error={fieldError("deadline")}>
                  {({ id, describedBy }) => <input id={id} type="date" className="input" value={form.deadline} onChange={(e) => set("deadline", e.target.value)} aria-describedby={describedBy} />}
                </Field>
              </div>
              <Field label="Tags" hint="Separate with commas.">
                {({ id, describedBy }) => <input id={id} className="input" value={form.tags} onChange={(e) => set("tags", e.target.value)} aria-describedby={describedBy} />}
              </Field>
              {isProject && group && (
                <div className="form-grid">
                  <Field label="Board column">
                    {({ id }) => (
                      <select id={id} className="select" value={form.columnKey ?? ""} onChange={(e) => set("columnKey", e.target.value || null)}>
                        {group.columns.map((c) => (
                          <option key={c.key} value={c.key}>
                            {c.name}
                          </option>
                        ))}
                      </select>
                    )}
                  </Field>
                  <Field label="Milestone">
                    {({ id }) => (
                      <select id={id} className="select" value={form.milestoneId ?? ""} onChange={(e) => set("milestoneId", e.target.value || null)}>
                        <option value="">None</option>
                        {group.milestones.map((m) => (
                          <option key={m.id} value={m.id}>
                            {m.title}
                          </option>
                        ))}
                      </select>
                    )}
                  </Field>
                </div>
              )}
            </div>
          </details>
          {task && task.assigneeId && me && task.assigneeId !== me.id && (
            <button type="button" className="btn btn-soft" onClick={() => nudge.mutate()} disabled={nudge.isPending}>
              <BellRing size={18} aria-hidden /> Send a friendly reminder
            </button>
          )}
        </form>
      )}
    </Dialog>
  );
}

function TaskHistoryView({ taskId }: { taskId: string }) {
  const { data, isLoading, error } = useQuery({ queryKey: ["taskHistory", taskId], queryFn: () => api.taskHistory(taskId) });
  if (isLoading) return <p className="muted">Loading history…</p>;
  if (error) return <p role="alert">{errorMessage(error)}</p>;
  if (!data) return null;
  return (
    <div className="stack">
      <section>
        <h3>Times it was done</h3>
        {data.completions.length === 0 ? (
          <p className="muted">Not done yet.</p>
        ) : (
          <ul className="stack-sm" style={{ paddingLeft: 18 }}>
            {data.completions.map((c) => (
              <li key={c.id}>
                {c.completedByName} · {new Date(c.completedAt).toLocaleString()}
                {c.occurrenceDue ? <span className="muted"> (was due {c.occurrenceDue})</span> : null}
              </li>
            ))}
          </ul>
        )}
      </section>
      <section>
        <h3>Changes</h3>
        <ul className="stack-sm" style={{ paddingLeft: 18 }}>
          {data.activity.map((a) => (
            <li key={a.id}>
              <span>{a.summary}</span>
              <span className="muted small"> · {new Date(a.at).toLocaleString()}</span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
