import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Plus, Repeat, Search } from "lucide-react";
import type { Category, RoutineTemplate, Task } from "@tendly/contracts";
import { api, errorMessage } from "../lib/api";
import { friendlyDate, localTimezone, todayIso } from "../lib/dates";
import { categoryMeta } from "../lib/categories";
import { keys, TASK_RELATED, useGroups, useInvalidate, useMe, useMembers, useTasks } from "../lib/queries";
import { CategoryPicker, GroupSelect, MemberSelect } from "../components/pickers";
import { TaskItem } from "../components/TaskItem";
import { TaskEditor } from "../components/TaskEditor";
import { Dialog, EmptyState, ErrorState, Field, Segmented, SkeletonList, Tip, useToast } from "../components/ui";

type View = "todo" | "routines" | "done";

export default function Tasks() {
  const [view, setView] = useState<View>("todo");
  const [category, setCategory] = useState<Category | null>(null);
  const [groupId, setGroupId] = useState<string | null>(null);
  const [assignee, setAssignee] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [editing, setEditing] = useState<Task | null>(null);
  const [creating, setCreating] = useState(false);
  const [gallery, setGallery] = useState(false);
  const { data: groups = [] } = useGroups();
  const { data: members = [] } = useMembers();
  const groupName = useMemo(() => new Map(groups.map((g) => [g.id, g.name])), [groups]);
  const q = useTasks({
    status: view === "done" ? "done" : "open",
    category: category ?? undefined,
    groupId: groupId ?? undefined,
    assigneeId: assignee ?? undefined,
    q: search || undefined,
  });
  const tasks = (q.data ?? []).filter((t) => (view === "routines" ? !!t.recurrence : view === "todo" ? true : true));
  const today = todayIso();

  const grouped = useMemo(() => {
    const out = new Map<string, Task[]>();
    for (const t of tasks) {
      const key = view === "done" ? "Done" : !t.dueDate ? "Someday" : t.dueDate < today ? "From earlier" : friendlyDate(t.dueDate, today);
      out.set(key, [...(out.get(key) ?? []), t]);
    }
    return [...out.entries()];
  }, [tasks, today, view]);

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Tasks & routines</h1>
          <p>Everything in one calm list. Filters are optional.</p>
        </div>
        <div className="row">
          <button type="button" className="btn" onClick={() => setGallery(true)}>
            <Repeat size={18} aria-hidden /> Add a routine
          </button>
          <button type="button" className="btn btn-primary" onClick={() => setCreating(true)}>
            <Plus size={18} aria-hidden /> New task
          </button>
        </div>
      </div>

      <Segmented
        label="Which tasks"
        value={view}
        onChange={setView}
        options={[
          { value: "todo", label: "To do" },
          { value: "routines", label: "Routines" },
          { value: "done", label: "Done" },
        ]}
      />

      <details className="disclosure card-quiet">
        <summary>Filter</summary>
        <div className="stack" style={{ marginTop: 8 }}>
          <div className="row" style={{ position: "relative" }}>
            <label htmlFor="task-search" className="visually-hidden">
              Search tasks
            </label>
            <Search size={18} aria-hidden style={{ position: "absolute", left: 12 }} />
            <input id="task-search" className="input" style={{ paddingLeft: 38 }} placeholder="Search" value={search} onChange={(e) => setSearch(e.target.value)} />
          </div>
          <CategoryPicker value={category} onChange={setCategory} allowAll label="Category" />
          <div className="form-grid">
            <Field label="Group">
              {({ id }) => <GroupSelect id={id} groups={groups} value={groupId} onChange={setGroupId} noneLabel="All groups and personal" />}
            </Field>
            <Field label="Person">
              {({ id }) => <MemberSelect id={id} members={members} value={assignee} onChange={setAssignee} noneLabel="Anyone" />}
            </Field>
          </div>
        </div>
      </details>

      {view === "routines" && (
        <Tip id="routines-intro" pose="happy">
          <p>Routines repeat on their own. Finishing one keeps a record and sets the next date. Use “Take turns” in a group so chores rotate fairly.</p>
        </Tip>
      )}

      {q.error && <ErrorState message={errorMessage(q.error)} onRetry={() => q.refetch()} />}
      {q.isLoading ? (
        <SkeletonList />
      ) : tasks.length === 0 ? (
        view === "routines" ? (
          <EmptyState title="No routines yet" action={<button className="btn btn-primary" onClick={() => setGallery(true)}>Browse routine templates</button>}>
            <p>Start from a template like dishes, laundry or recycling and adjust it.</p>
          </EmptyState>
        ) : view === "done" ? (
          <EmptyState title="Nothing finished yet" pose="sleepy" />
        ) : (
          <EmptyState title="Your list is empty" pose="celebrate">
            <p>Add anything that's on your mind — small steps count.</p>
          </EmptyState>
        )
      ) : (
        grouped.map(([label, list]) => (
          <section key={label} aria-label={label}>
            <h2 className="section-title">{label}</h2>
            <ul className="task-list">
              {list.map((t) => (
                <TaskItem key={t.id} task={t} onOpen={setEditing} showGroup={t.groupId ? groupName.get(t.groupId) : undefined} />
              ))}
            </ul>
          </section>
        ))
      )}

      <TaskEditor open={!!editing} task={editing ?? undefined} onClose={() => setEditing(null)} />
      <TaskEditor open={creating} defaults={{ groupId, category: category ?? undefined }} onClose={() => setCreating(false)} />
      <TemplateGallery open={gallery} onClose={() => setGallery(false)} defaultGroupId={groupId} />
    </div>
  );
}

export function TemplateGallery({ open, onClose, defaultGroupId }: { open: boolean; onClose: () => void; defaultGroupId?: string | null }) {
  const { data: templates = [] } = useQuery({ queryKey: keys.templates, queryFn: api.templates, enabled: open });
  const { data: groups = [] } = useGroups();
  const { data: members = [] } = useMembers();
  const me = useMe();
  const [chosen, setChosen] = useState<RoutineTemplate | null>(null);
  const [groupId, setGroupId] = useState<string | null>(defaultGroupId ?? null);
  const [rotation, setRotation] = useState<string[]>([]);
  const [due, setDue] = useState(todayIso());
  const invalidate = useInvalidate();
  const toast = useToast();
  const group = groups.find((g) => g.id === groupId);
  const groupMembers = group ? members.filter((m) => group.memberIds.includes(m.id)) : [];
  const add = useMutation({
    mutationFn: () => api.useTemplate(chosen!.key, { groupId: groupId ?? undefined, rotation: rotation.length ? rotation : undefined, dueDate: due, timezone: localTimezone() }),
    onSuccess: (t) => {
      invalidate(...TASK_RELATED);
      toast.show(`Added routine “${t.title}”`);
      setChosen(null);
      onClose();
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  return (
    <Dialog
      open={open}
      onClose={() => {
        setChosen(null);
        onClose();
      }}
      title={chosen ? `Add “${chosen.title}”` : "Routine templates"}
      wide
      footer={
        chosen ? (
          <>
            <button type="button" className="btn" onClick={() => setChosen(null)}>
              Back
            </button>
            <button type="button" className="btn btn-primary" onClick={() => add.mutate()} disabled={add.isPending}>
              Add routine
            </button>
          </>
        ) : undefined
      }
    >
      {!chosen ? (
        <div className="grid-3">
          {templates.map((t) => {
            const m = categoryMeta(t.category);
            const Icon = m?.icon;
            return (
              <button key={t.key} type="button" className={`card cat cat-${t.category}`} style={{ textAlign: "left", cursor: "pointer", borderLeft: "6px solid var(--c-line)", font: "inherit", color: "inherit" }} onClick={() => setChosen(t)}>
                <span className="row" style={{ fontWeight: 700 }}>
                  {Icon && <Icon size={18} aria-hidden />} {t.title}
                </span>
                <span className="small muted" style={{ display: "block", marginTop: 4 }}>
                  {t.recurrenceLabel} · ~{t.durationMinutes} min
                </span>
              </button>
            );
          })}
        </div>
      ) : (
        <div className="stack">
          <p className="muted">{chosen.tip}</p>
          <p className="small">
            Steps: {chosen.checklist.join(" · ")}. Repeats: {chosen.recurrenceLabel}. You can change all of this afterwards.
          </p>
          <div className="form-grid">
            <Field label="Who is it for?">
              {({ id }) => (
                <GroupSelect
                  id={id}
                  groups={groups.filter((g) => me && g.memberIds.includes(me.id) && !g.archived)}
                  value={groupId}
                  onChange={(g) => {
                    setGroupId(g);
                    setRotation([]);
                  }}
                />
              )}
            </Field>
            <Field label="First time">
              {({ id }) => <input id={id} type="date" className="input" value={due} onChange={(e) => setDue(e.target.value)} />}
            </Field>
          </div>
          {group && (
            <fieldset>
              <legend>Take turns</legend>
              <div className="chips" style={{ marginTop: 6 }}>
                {groupMembers.map((m) => (
                  <button key={m.id} type="button" className="chip" aria-pressed={rotation.includes(m.id)} onClick={() => setRotation(rotation.includes(m.id) ? rotation.filter((x) => x !== m.id) : [...rotation, m.id])}>
                    {rotation.includes(m.id) ? `${rotation.indexOf(m.id) + 1}. ` : ""}
                    {m.displayName}
                  </button>
                ))}
              </div>
            </fieldset>
          )}
        </div>
      )}
    </Dialog>
  );
}
