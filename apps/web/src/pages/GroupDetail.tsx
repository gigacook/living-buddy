import { useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { ArrowLeft, Diamond, LogOut, Plus, Repeat } from "lucide-react";
import type { Group, Task } from "@tendly/contracts";
import { api, ApiError, errorMessage } from "../lib/api";
import { addDays, daysBetween, friendlyDate, todayIso } from "../lib/dates";
import { keys, TASK_RELATED, useInvalidate, useMe, useMembers, useTasks } from "../lib/queries";
import { Avatar, CategoryBadge, Dialog, EmptyState, ErrorState, Field, Segmented, SkeletonList, useToast } from "../components/ui";
import { TaskItem } from "../components/TaskItem";
import { TaskEditor, type TaskDraftDefaults } from "../components/TaskEditor";
import { TemplateGallery } from "./Tasks";
import { KIND_LABELS } from "./Groups";

export default function GroupDetail() {
  const { id = "" } = useParams();
  const g = useQuery({ queryKey: keys.group(id), queryFn: () => api.group(id) });
  const me = useMe();
  if (g.isLoading) return <SkeletonList />;
  if (g.error || !g.data) return <ErrorState message={errorMessage(g.error)} />;
  const group = g.data;
  if (me && !group.memberIds.includes(me.id)) return <ErrorState message="You're not in this group. Join it from the Groups page to see its items." />;
  return <GroupView group={group} />;
}

function GroupView({ group }: { group: Group }) {
  const [tab, setTab] = useState<"main" | "timeline" | "people" | "activity">("main");
  const [editing, setEditing] = useState<Task | null>(null);
  const [creating, setCreating] = useState<TaskDraftDefaults | null>(null);
  const [gallery, setGallery] = useState(false);
  const tasks = useTasks({ groupId: group.id, status: "all" });
  const isProject = group.mode === "project";
  return (
    <div className="stack">
      <Link to="/groups" className="btn btn-ghost" style={{ alignSelf: "flex-start" }}>
        <ArrowLeft size={18} aria-hidden /> Groups
      </Link>
      <div className="page-head">
        <div>
          <h1>{group.name}</h1>
          <p>
            {KIND_LABELS[group.kind]} · {isProject ? "Project" : "Ongoing household"}
            {group.goal ? ` · Goal: ${group.goal}` : ""}
            {group.endDate ? ` · Finish by ${friendlyDate(group.endDate)}` : ""}
          </p>
        </div>
        <div className="row">
          {!isProject && (
            <button type="button" className="btn" onClick={() => setGallery(true)}>
              <Repeat size={18} aria-hidden /> Add routine
            </button>
          )}
          <button type="button" className="btn btn-primary" onClick={() => setCreating({ groupId: group.id, columnKey: isProject ? group.columns[0]?.key : null })}>
            <Plus size={18} aria-hidden /> Add task
          </button>
        </div>
      </div>
      <Segmented
        label="Group sections"
        value={tab}
        onChange={setTab}
        options={[
          { value: "main", label: isProject ? "Board" : "Chores" },
          ...(isProject ? [{ value: "timeline" as const, label: "Timeline" }] : []),
          { value: "people", label: "People" },
          { value: "activity", label: "Activity" },
        ]}
      />
      {tasks.error && <ErrorState message={errorMessage(tasks.error)} />}
      {tab === "main" && (isProject ? <Board group={group} tasks={tasks.data ?? []} onOpen={setEditing} onAdd={(col) => setCreating({ groupId: group.id, columnKey: col })} /> : <Household group={group} tasks={tasks.data ?? []} onOpen={setEditing} />)}
      {tab === "timeline" && <Timeline group={group} tasks={tasks.data ?? []} onOpen={setEditing} />}
      {tab === "people" && <People group={group} />}
      {tab === "activity" && <ActivityFeed groupId={group.id} />}
      <TaskEditor open={!!editing} task={editing ?? undefined} onClose={() => setEditing(null)} />
      <TaskEditor open={!!creating} defaults={creating ?? {}} onClose={() => setCreating(null)} />
      <TemplateGallery open={gallery} onClose={() => setGallery(false)} defaultGroupId={group.id} />
    </div>
  );
}

function Household({ group, tasks, onOpen }: { group: Group; tasks: Task[]; onOpen: (t: Task) => void }) {
  const { data: members = [] } = useMembers();
  const open = tasks.filter((t) => !t.completedAt);
  const byPerson = useMemo(() => {
    const m = new Map<string, Task[]>();
    for (const t of open) m.set(t.assigneeId ?? "", [...(m.get(t.assigneeId ?? "") ?? []), t]);
    return m;
  }, [open]);
  if (open.length === 0)
    return (
      <EmptyState title="No shared chores yet">
        <p>Add a routine like dishes or recycling, and choose who takes turns.</p>
      </EmptyState>
    );
  return (
    <div className="grid-2">
      {[...group.memberIds, ""].map((pid) => {
        const list = byPerson.get(pid) ?? [];
        if (list.length === 0) return null;
        const person = members.find((m) => m.id === pid);
        return (
          <section key={pid || "none"} className="card stack-sm" aria-label={person ? `${person.displayName}'s items` : "Unassigned"}>
            <h2 className="row">
              {person ? <Avatar member={person} /> : null}
              {person ? person.displayName : "Up for grabs"}
            </h2>
            <ul className="task-list">
              {list.map((t) => (
                <TaskItem key={t.id} task={t} onOpen={onOpen} />
              ))}
            </ul>
          </section>
        );
      })}
    </div>
  );
}

function Board({ group, tasks, onOpen, onAdd }: { group: Group; tasks: Task[]; onOpen: (t: Task) => void; onAdd: (col: string) => void }) {
  const invalidate = useInvalidate();
  const toast = useToast();
  const { data: members = [] } = useMembers();
  const [dropCol, setDropCol] = useState<string | null>(null);
  const [columnsOpen, setColumnsOpen] = useState(false);
  const move = useMutation({
    mutationFn: ({ t, col }: { t: Task; col: string }) => api.moveTask(t.id, t.version, col),
    onSuccess: (t) => {
      invalidate(...TASK_RELATED);
      toast.show(`Moved “${t.title}” to ${group.columns.find((c) => c.key === t.columnKey)?.name}`);
    },
    onError: (e) => {
      invalidate("tasks");
      toast.show(e instanceof ApiError && e.status === 409 ? "That card changed elsewhere — refreshed the board." : errorMessage(e));
    },
  });
  const cols = group.columns;
  return (
    <div className="stack-sm">
      <div className="row-between">
        <p className="small muted" style={{ margin: 0 }}>
          Drag cards between columns, or use the “Move to” menu on each card.
        </p>
        <button type="button" className="btn btn-sm" onClick={() => setColumnsOpen(true)}>
          Edit columns
        </button>
      </div>
      <div className="board">
        {cols.map((c) => {
          const list = tasks.filter((t) => t.columnKey === c.key).sort((a, b) => a.position - b.position);
          return (
            <section
              key={c.key}
              className="column"
              aria-label={`${c.name}, ${list.length} cards`}
              data-drop={dropCol === c.key}
              onDragOver={(e) => {
                e.preventDefault();
                setDropCol(c.key);
              }}
              onDragLeave={() => setDropCol(null)}
              onDrop={(e) => {
                e.preventDefault();
                setDropCol(null);
                const t = tasks.find((x) => x.id === e.dataTransfer.getData("text/plain"));
                if (t && t.columnKey !== c.key) move.mutate({ t, col: c.key });
              }}
            >
              <div className="column-head">
                <h2 style={{ fontSize: "1rem", margin: 0 }}>{c.name}</h2>
                <span className="badge badge-plain">{list.length}</span>
              </div>
              {list.map((t) => {
                const who = members.find((m) => m.id === t.assigneeId);
                return (
                  <article key={t.id} className={`kanban-card cat cat-${t.category}`} draggable onDragStart={(e) => e.dataTransfer.setData("text/plain", t.id)} aria-label={t.title}>
                    <button type="button" className="task-main" onClick={() => onOpen(t)}>
                      <span className="task-title">{t.title}</span>
                    </button>
                    <div className="task-meta">
                      <CategoryBadge category={t.category} />
                      {t.dueDate && <span>{friendlyDate(t.dueDate)}</span>}
                      {who && <span className="row" style={{ gap: 4 }}><Avatar member={who} size={22} />{who.displayName}</span>}
                    </div>
                    <label className="small">
                      <span className="visually-hidden">Move “{t.title}” to</span>
                      <select className="select" style={{ minHeight: 36 }} value={t.columnKey ?? ""} onChange={(e) => move.mutate({ t, col: e.target.value })} aria-label={`Move “${t.title}” to column`}>
                        {cols.map((x) => (
                          <option key={x.key} value={x.key}>
                            {x.key === t.columnKey ? `In ${x.name}` : `Move to ${x.name}`}
                          </option>
                        ))}
                      </select>
                    </label>
                  </article>
                );
              })}
              <button type="button" className="btn btn-ghost btn-sm" onClick={() => onAdd(c.key)}>
                <Plus size={16} aria-hidden /> Add card
              </button>
            </section>
          );
        })}
      </div>
      <ColumnsDialog open={columnsOpen} onClose={() => setColumnsOpen(false)} group={group} />
    </div>
  );
}

function ColumnsDialog({ open, onClose, group }: { open: boolean; onClose: () => void; group: Group }) {
  const [cols, setCols] = useState(group.columns.map((c) => ({ key: c.key as string | undefined, name: c.name })));
  const [error, setError] = useState<string | null>(null);
  const invalidate = useInvalidate();
  const save = useMutation({
    mutationFn: () => api.updateGroup(group.id, { columns: cols.map((c) => ({ key: c.key, name: c.name })) }),
    onSuccess: () => {
      invalidate("groups", "tasks");
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Board columns"
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="btn btn-primary" onClick={() => save.mutate()}>
            Save columns
          </button>
        </>
      }
    >
      <div className="stack-sm">
        {error && (
          <div className="banner banner-danger" role="alert">
            {error}
          </div>
        )}
        <p className="small muted">Rename, add or remove columns. Cards in a removed column move to the first column. Keep a “Done” column.</p>
        {cols.map((c, i) => (
          <div key={i} className="row">
            <label className="visually-hidden" htmlFor={`col-${i}`}>
              Column {i + 1} name
            </label>
            <input id={`col-${i}`} className="input" style={{ flex: 1 }} value={c.name} onChange={(e) => setCols(cols.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)))} />
            <button type="button" className="btn btn-ghost btn-sm" disabled={c.key === "done"} onClick={() => setCols(cols.filter((_, j) => j !== i))}>
              Remove
            </button>
          </div>
        ))}
        <button type="button" className="btn btn-sm" onClick={() => setCols([...cols.slice(0, -1), { key: undefined, name: "New column" }, ...cols.slice(-1)])}>
          <Plus size={16} aria-hidden /> Add column
        </button>
      </div>
    </Dialog>
  );
}

function Timeline({ group, tasks, onOpen }: { group: Group; tasks: Task[]; onOpen: (t: Task) => void }) {
  const today = todayIso();
  const invalidate = useInvalidate();
  const toast = useToast();
  const [title, setTitle] = useState("");
  const [due, setDue] = useState("");
  const dated = tasks.filter((t) => t.dueDate || t.startDate);
  const allDates = [...dated.flatMap((t) => [t.startDate, t.dueDate]), ...group.milestones.map((m) => m.dueDate), group.startDate, group.endDate, today].filter(Boolean) as string[];
  const start = allDates.length ? allDates.reduce((a, b) => (a < b ? a : b)) : today;
  const endRaw = allDates.length ? allDates.reduce((a, b) => (a > b ? a : b)) : addDays(today, 30);
  const end = daysBetween(start, endRaw) < 14 ? addDays(start, 14) : endRaw;
  const span = Math.max(1, daysBetween(start, end) + 1);
  const pos = (d: string) => `${(daysBetween(start, d) / span) * 100}%`;
  const addMs = useMutation({
    mutationFn: () => api.addMilestone(group.id, { title, dueDate: due || undefined }),
    onSuccess: () => {
      setTitle("");
      setDue("");
      invalidate("groups");
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  const toggleMs = useMutation({
    mutationFn: (m: Group["milestones"][number]) => api.updateMilestone(group.id, m.id, { title: m.title, dueDate: m.dueDate ?? undefined, done: !m.done }),
    onSuccess: () => invalidate("groups"),
  });
  return (
    <div className="stack">
      {/* Focusable so keyboard users can scroll the wide timeline (WCAG 2.1.1). */}
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex */}
      <div className="timeline" role="region" aria-label="Project timeline" tabIndex={0}>
        <div className="timeline-inner">
          <div className="timeline-scale" aria-hidden>
            <span>{friendlyDate(start)}</span>
            <span>{friendlyDate(addDays(start, Math.floor(span / 2)))}</span>
            <span>{friendlyDate(end)}</span>
          </div>
          <div style={{ position: "relative" }}>
            {today >= start && today <= end && <div className="timeline-today" style={{ left: pos(today) }} aria-hidden />}
            {group.milestones.map((m) =>
              m.dueDate ? (
                <div key={m.id} className="timeline-row">
                  <span className="milestone-marker" style={{ left: `calc(${pos(m.dueDate)} - 8px)`, opacity: m.done ? 0.4 : 1 }} aria-hidden />
                  <span style={{ position: "absolute", left: `calc(${pos(m.dueDate)} + 14px)`, top: 8, fontWeight: 700, fontSize: "0.85rem" }}>
                    ◆ {m.title} ({friendlyDate(m.dueDate)})
                  </span>
                </div>
              ) : null,
            )}
            {dated.map((t) => {
              const s = t.startDate ?? t.dueDate!;
              const e = t.dueDate ?? t.startDate!;
              const width = `max(${((daysBetween(s, e) + 1) / span) * 100}%, 120px)`;
              return (
                <div key={t.id} className="timeline-row">
                  <button type="button" className={`timeline-bar cat cat-${t.category}`} style={{ left: pos(s), width, cursor: "pointer", opacity: t.completedAt ? 0.55 : 1 }} onClick={() => onOpen(t)}>
                    {t.completedAt ? "✓ " : ""}
                    {t.title}
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      </div>
      <section className="card stack-sm" aria-labelledby="ms-heading">
        <h2 id="ms-heading" className="row">
          <Diamond size={18} aria-hidden /> Milestones
        </h2>
        <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
          {group.milestones.map((m) => (
            <li key={m.id}>
              <label className="check">
                <input type="checkbox" checked={m.done} onChange={() => toggleMs.mutate(m)} />
                {m.title} {m.dueDate && <span className="muted small">· {friendlyDate(m.dueDate)}</span>}
              </label>
            </li>
          ))}
        </ul>
        <form
          className="row"
          onSubmit={(e) => {
            e.preventDefault();
            if (title.trim()) addMs.mutate();
          }}
        >
          <label htmlFor="ms-title" className="visually-hidden">
            Milestone name
          </label>
          <input id="ms-title" className="input" style={{ flex: 2, minWidth: 160 }} placeholder="New milestone" value={title} onChange={(e) => setTitle(e.target.value)} />
          <label htmlFor="ms-date" className="visually-hidden">
            Milestone date
          </label>
          <input id="ms-date" type="date" className="input" style={{ flex: 1, minWidth: 140 }} value={due} onChange={(e) => setDue(e.target.value)} />
          <button type="submit" className="btn" disabled={!title.trim()}>
            Add
          </button>
        </form>
      </section>
    </div>
  );
}

function People({ group }: { group: Group }) {
  const { data: members = [] } = useMembers();
  const me = useMe();
  const invalidate = useInvalidate();
  const toast = useToast();
  const update = useMutation({
    mutationFn: (ids: string[]) => api.updateGroup(group.id, { memberIds: ids }),
    onSuccess: () => invalidate("groups"),
    onError: (e) => toast.show(errorMessage(e)),
  });
  const leave = useMutation({
    mutationFn: () => api.leaveGroup(group.id),
    onSuccess: () => {
      invalidate("groups", "tasks");
      toast.show(`You left “${group.name}”. Your open items there are now unassigned.`);
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  return (
    <div className="card stack">
      <p className="small muted">Anyone in the group can see and update its items. Changes are recorded with the name of who made them.</p>
      <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {members.map((m) => {
          const inGroup = group.memberIds.includes(m.id);
          return (
            <li key={m.id} className="row-between">
              <span className="row">
                <Avatar member={m} /> {m.displayName}
                {m.id === me?.id ? " (you)" : ""}
              </span>
              {m.id !== me?.id && (
                <button type="button" className="btn btn-sm" onClick={() => update.mutate(inGroup ? group.memberIds.filter((x) => x !== m.id) : [...group.memberIds, m.id])}>
                  {inGroup ? "Remove" : "Add"}
                </button>
              )}
            </li>
          );
        })}
      </ul>
      <Field label="Group name">
        {({ id }) => (
          <input
            id={id}
            className="input"
            defaultValue={group.name}
            onBlur={(e) => e.target.value.trim() && e.target.value !== group.name && api.updateGroup(group.id, { name: e.target.value }).then(() => invalidate("groups"))}
          />
        )}
      </Field>
      <button type="button" className="btn btn-ghost btn-danger" style={{ alignSelf: "flex-start" }} onClick={() => leave.mutate()}>
        <LogOut size={18} aria-hidden /> Leave group
      </button>
    </div>
  );
}

export function ActivityFeed({ groupId }: { groupId?: string }) {
  const { data = [], isLoading } = useQuery({ queryKey: keys.activity({ groupId }), queryFn: () => api.activity({ groupId, limit: 100 }) });
  if (isLoading) return <SkeletonList />;
  if (data.length === 0) return <EmptyState title="Nothing has happened here yet" pose="sleepy" />;
  return (
    <ul className="card stack-sm" style={{ listStyle: "none", margin: 0 }}>
      {data.map((a) => (
        <li key={a.id} className="row-between">
          <span>{a.summary}</span>
          <span className="small muted">{new Date(a.at).toLocaleString()}</span>
        </li>
      ))}
    </ul>
  );
}
