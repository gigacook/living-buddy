import { Check, Clock, Repeat, Users } from "lucide-react";
import type { Task } from "@tendly/contracts";
import { useMutation } from "@tanstack/react-query";
import { api, errorMessage } from "../lib/api";
import { gentleDue, todayIso } from "../lib/dates";
import { useInvalidate, useMemberMap, TASK_RELATED } from "../lib/queries";
import { Avatar, CategoryBadge, useToast } from "./ui";

export function useCompleteTask() {
  const invalidate = useInvalidate();
  const toast = useToast();
  return useMutation({
    mutationFn: (t: Task) => (t.completedAt ? api.reopenTask(t.id).then((task) => ({ task, nextDueDate: null, nextAssigneeId: null })) : api.completeTask(t.id)),
    onSuccess: (r, t) => {
      invalidate(...TASK_RELATED);
      if (t.completedAt) {
        toast.show(`Reopened “${t.title}”`);
      } else if (r.nextDueDate) {
        toast.show(`Nice. “${t.title}” is next due ${gentleDue(r.nextDueDate).toLowerCase()}.`);
      } else {
        toast.show(`Done: “${t.title}”`, { label: "Undo", run: () => api.reopenTask(t.id).then(() => invalidate(...TASK_RELATED)) });
      }
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
}

export function TaskItem({ task, onOpen, showGroup }: { task: Task; onOpen: (t: Task) => void; showGroup?: string }) {
  const members = useMemberMap();
  const complete = useCompleteTask();
  const assignee = task.assigneeId ? members.get(task.assigneeId) : undefined;
  const today = todayIso();
  const late = task.dueDate && task.dueDate < today && !task.completedAt;
  const doneSubs = task.subtasks.filter((s) => s.done).length;
  return (
    <li className={`task cat cat-${task.category}${task.completedAt ? " done" : ""}`}>
      <button
        type="button"
        className="complete-btn"
        aria-pressed={!!task.completedAt}
        aria-label={task.completedAt ? `Mark “${task.title}” as not done` : `Mark “${task.title}” as done`}
        onClick={() => complete.mutate(task)}
        disabled={complete.isPending}
      >
        {task.completedAt ? <Check size={20} aria-hidden /> : null}
      </button>
      <button type="button" className="task-main" onClick={() => onOpen(task)}>
        <span className="task-title">{task.title}</span>
        <span className="task-meta">
          <CategoryBadge category={task.category} />
          {task.dueDate && (
            <span className={late ? "badge badge-warn" : undefined}>
              {gentleDue(task.dueDate, today)}
              {task.dueTime ? ` · ${task.dueTime}` : ""}
            </span>
          )}
          {task.recurrenceLabel && (
            <span className="row" style={{ gap: 4 }}>
              <Repeat size={14} aria-hidden />
              {task.recurrenceLabel}
            </span>
          )}
          {task.durationMinutes && (
            <span className="row" style={{ gap: 4 }}>
              <Clock size={14} aria-hidden />
              {task.durationMinutes} min
            </span>
          )}
          {task.subtasks.length > 0 && (
            <span>
              {doneSubs}/{task.subtasks.length} steps
            </span>
          )}
          {task.rotation.length > 1 && (
            <span className="row" style={{ gap: 4 }}>
              <Users size={14} aria-hidden />
              Takes turns
            </span>
          )}
          {showGroup && <span>{showGroup}</span>}
          {task.priority === "high" && <span className="badge badge-plain">Important</span>}
        </span>
      </button>
      <span className="row" style={{ gap: 6 }}>
        {assignee && (
          <>
            <Avatar member={assignee} />
            <span className="visually-hidden">Assigned to {assignee.displayName}</span>
          </>
        )}
      </span>
    </li>
  );
}
