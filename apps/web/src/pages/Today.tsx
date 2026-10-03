import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { CalendarDays, Check, Hourglass, Plus, Timer } from "lucide-react";
import type { Task } from "@tendly/contracts";
import { api, errorMessage } from "../lib/api";
import { addDays, countdownLabel, formatDuration, gentleDue, localTimezone, longDate, timeOf, todayIso } from "../lib/dates";
import { keys, TASK_RELATED, useGroups, useInvalidate, useMe, useTasks } from "../lib/queries";
import { useTimer } from "../lib/useTimer";
import { phaseLabel } from "../lib/timer";
import { CategoryBadge, EmptyState, ErrorState, SkeletonList, Tip, useToast } from "../components/ui";
import { TaskItem, useCompleteTask } from "../components/TaskItem";
import { TaskEditor } from "../components/TaskEditor";
import { UsageCard } from "../components/UsageCard";
import { Mascot } from "../components/Mascot";
import { usePrefs } from "../lib/prefs";

function greeting(): string {
  const h = new Date().getHours();
  if (h < 5) return "Hi";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

const PRIORITY_RANK = { high: 0, normal: 1, low: 2 } as const;

/** Picks one obvious next thing: earliest-timed item due today, then important, then the oldest leftover. */
export function pickNext(tasks: Task[], meId: string | undefined, today: string): Task | undefined {
  const mine = tasks.filter((t) => !t.completedAt && (!t.assigneeId || t.assigneeId === meId) && t.dueDate && t.dueDate <= today);
  return [...mine].sort((a, b) => {
    const at = a.dueDate === today ? 0 : 1;
    const bt = b.dueDate === today ? 0 : 1;
    if (at !== bt) return at - bt;
    const pr = PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority];
    if (pr) return pr;
    return (a.dueTime ?? "99").localeCompare(b.dueTime ?? "99") || (a.dueDate ?? "").localeCompare(b.dueDate ?? "");
  })[0];
}

export function Today() {
  const me = useMe();
  const today = todayIso();
  const prefs = usePrefs();
  const tasksQ = useTasks({ status: "open", dueBefore: addDays(today, 3) });
  const { data: groups = [] } = useGroups();
  const groupName = useMemo(() => new Map(groups.map((g) => [g.id, g.name])), [groups]);
  const [editing, setEditing] = useState<Task | null>(null);
  const [creating, setCreating] = useState(false);
  const [quick, setQuick] = useState("");
  const invalidate = useInvalidate();
  const toast = useToast();
  const complete = useCompleteTask();
  const timer = useTimer();
  const range = { from: today, to: addDays(today, 1), includeTasks: false };
  const events = useQuery({ queryKey: keys.occurrences(range), queryFn: () => api.occurrences(range) });
  const countdowns = useQuery({ queryKey: keys.countdowns, queryFn: api.countdowns });

  const quickAdd = useMutation({
    mutationFn: () => api.createTask({ title: quick, dueDate: today, timezone: localTimezone() }),
    onSuccess: (t) => {
      setQuick("");
      invalidate(...TASK_RELATED);
      toast.show(`Added “${t.title}” for today`);
    },
    onError: (e) => toast.show(errorMessage(e)),
  });

  const tasks = tasksQ.data ?? [];
  const mineFirst = tasks.filter((t) => !t.assigneeId || t.assigneeId === me?.id);
  const next = pickNext(tasks, me?.id, today);
  const dueToday = mineFirst.filter((t) => t.dueDate === today && t.id !== next?.id);
  const earlier = mineFirst.filter((t) => t.dueDate && t.dueDate < today && t.id !== next?.id);
  const soon = mineFirst.filter((t) => t.dueDate && t.dueDate > today);
  const othersToday = tasks.filter((t) => t.assigneeId && t.assigneeId !== me?.id && t.dueDate && t.dueDate <= today);
  const timerActive = timer.data && ["running", "paused", "phase_complete"].includes(timer.data.state.status);

  return (
    <div className="stack">
      <header className="hero">
        <div className="row-between">
          <div>
            <p className="muted" style={{ margin: 0 }}>
              {longDate(today)}
            </p>
            <h1>
              {greeting()}
              {me ? `, ${me.displayName}` : ""}
            </h1>
            <p className="muted" style={{ margin: 0 }}>
              {next ? "Here's one thing to start with. Everything else can wait its turn." : "Nothing pressing right now."}
            </p>
          </div>
          {!prefs.quiet && (
            <span className="hero-mascot">
              <Mascot pose={next ? "happy" : "celebrate"} size={88} />
            </span>
          )}
        </div>
      </header>

      <Tip id="today-intro">
        <p>
          <strong>Tip:</strong> Today shows one next thing. Tap the circle when it's done — repeating chores simply move to their next date, no streaks to lose.
        </p>
      </Tip>

      {tasksQ.error && <ErrorState message={errorMessage(tasksQ.error)} onRetry={() => tasksQ.refetch()} />}
      {tasksQ.isLoading && <SkeletonList rows={2} />}

      {next && (
        <section aria-labelledby="next-heading" className={`next-card cat cat-${next.category}`}>
          <h2 id="next-heading" className="section-title" style={{ margin: 0 }}>
            Next up
          </h2>
          <p className="next-title">{next.title}</p>
          <div className="row" style={{ marginBottom: 16 }}>
            <CategoryBadge category={next.category} />
            {next.dueDate && <span className="badge badge-plain">{gentleDue(next.dueDate, today)}{next.dueTime ? ` · ${next.dueTime}` : ""}</span>}
            {next.durationMinutes && <span className="badge badge-plain">About {next.durationMinutes} min</span>}
            {next.groupId && <span className="badge badge-plain">{groupName.get(next.groupId)}</span>}
          </div>
          <div className="row">
            <button type="button" className="btn btn-primary btn-lg" onClick={() => complete.mutate(next)} disabled={complete.isPending}>
              <Check size={20} aria-hidden /> Done
            </button>
            <Link
              className="btn btn-lg"
              to={`/focus?task=${next.id}&minutes=${Math.min(next.durationMinutes ?? 25, 90)}`}
            >
              <Timer size={20} aria-hidden /> Start a focus timer
            </Link>
            <button type="button" className="btn btn-ghost" onClick={() => setEditing(next)}>
              Details
            </button>
          </div>
        </section>
      )}

      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          if (quick.trim()) quickAdd.mutate();
        }}
      >
        <label htmlFor="quick-add" className="visually-hidden">
          Add something for today
        </label>
        <input id="quick-add" className="input" style={{ flex: 1, minWidth: 200 }} placeholder="Add something for today…" value={quick} onChange={(e) => setQuick(e.target.value)} maxLength={200} />
        <button type="submit" className="btn btn-primary" disabled={!quick.trim() || quickAdd.isPending}>
          <Plus size={18} aria-hidden /> Add
        </button>
        <button type="button" className="btn" onClick={() => setCreating(true)}>
          More details
        </button>
      </form>

      <div className="grid-2">
        <div className="stack">
          {dueToday.length > 0 && (
            <section aria-labelledby="today-list">
              <h2 id="today-list" className="section-title">
                Also today
              </h2>
              <ul className="task-list">
                {dueToday.map((t) => (
                  <TaskItem key={t.id} task={t} onOpen={setEditing} showGroup={t.groupId ? groupName.get(t.groupId) : undefined} />
                ))}
              </ul>
            </section>
          )}
          {earlier.length > 0 && (
            <details className="disclosure card-quiet">
              <summary>
                From earlier ({earlier.length}) <span className="muted small">— whenever you're ready</span>
              </summary>
              <ul className="task-list" style={{ marginTop: 8 }}>
                {earlier.map((t) => (
                  <TaskItem key={t.id} task={t} onOpen={setEditing} showGroup={t.groupId ? groupName.get(t.groupId) : undefined} />
                ))}
              </ul>
            </details>
          )}
          {soon.length > 0 && (
            <details className="disclosure card-quiet">
              <summary>Coming up in the next few days ({soon.length})</summary>
              <ul className="task-list" style={{ marginTop: 8 }}>
                {soon.map((t) => (
                  <TaskItem key={t.id} task={t} onOpen={setEditing} />
                ))}
              </ul>
            </details>
          )}
          {othersToday.length > 0 && (
            <details className="disclosure card-quiet">
              <summary>Others are handling ({othersToday.length})</summary>
              <ul className="task-list" style={{ marginTop: 8 }}>
                {othersToday.map((t) => (
                  <TaskItem key={t.id} task={t} onOpen={setEditing} showGroup={t.groupId ? groupName.get(t.groupId) : undefined} />
                ))}
              </ul>
            </details>
          )}
          {!tasksQ.isLoading && !next && dueToday.length === 0 && earlier.length === 0 && (
            <EmptyState title="All clear for today" pose="celebrate">
              <p>Enjoy the space. Add something above if it's on your mind.</p>
            </EmptyState>
          )}
        </div>

        <div className="stack">
          {timerActive && timer.data && (
            <Link to="/focus" className="card row" style={{ textDecoration: "none", color: "inherit" }}>
              <Timer size={22} aria-hidden />
              <span style={{ flex: 1 }}>
                <strong>{phaseLabel(timer.data)}</strong>
                {timer.data.state.label ? ` · ${timer.data.state.label}` : ""}
                <br />
                <span className="muted">{timer.data.state.status === "paused" ? "Paused" : timer.data.state.status === "phase_complete" ? "Ready for the next part" : `${formatDuration(timer.remaining)} left`}</span>
              </span>
            </Link>
          )}
          <section className="card" aria-labelledby="events-today">
            <h2 id="events-today" className="row">
              <CalendarDays size={20} aria-hidden /> On the calendar today
            </h2>
            {events.isLoading ? (
              <p className="muted">Loading…</p>
            ) : (events.data ?? []).length === 0 ? (
              <p className="muted">No events today.</p>
            ) : (
              <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
                {(events.data ?? []).map((o) => (
                  <li key={`${o.eventId}-${o.instanceKey}`} className="row">
                    <span className="badge badge-plain" style={{ minWidth: 64, justifyContent: "center" }}>
                      {o.allDay ? "All day" : timeOf(new Date(o.start))}
                    </span>
                    <span style={{ flex: 1 }}>{o.title}</span>
                    <CategoryBadge category={o.category} />
                  </li>
                ))}
              </ul>
            )}
          </section>
          {(countdowns.data ?? []).length > 0 && (
            <section className="card" aria-labelledby="countdowns-today">
              <h2 id="countdowns-today" className="row">
                <Hourglass size={20} aria-hidden /> Counting down
              </h2>
              <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
                {(countdowns.data ?? []).slice(0, 4).map((c) => (
                  <li key={c.id} className="row-between">
                    <span>{c.title}</span>
                    <strong>{countdownLabel(new Date(c.targetAt))}</strong>
                  </li>
                ))}
              </ul>
            </section>
          )}
          {me?.prefs.showUsageCard && <UsageCard compact />}
        </div>
      </div>

      <TaskEditor open={!!editing} task={editing ?? undefined} onClose={() => setEditing(null)} />
      <TaskEditor open={creating} defaults={{ dueDate: today, title: quick }} onClose={() => setCreating(false)} />
    </div>
  );
}
