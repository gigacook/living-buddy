import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { keys } from "../lib/queries";
import { EmptyState, SkeletonList } from "../components/ui";

export default function Notifications() {
  const qc = useQueryClient();
  const { data = [], isLoading } = useQuery({ queryKey: keys.notifications, queryFn: api.notifications });
  const readAll = useMutation({ mutationFn: api.markAllRead, onSuccess: () => qc.invalidateQueries({ queryKey: keys.notifications }) });
  const read = useMutation({ mutationFn: (id: string) => api.markRead(id), onSuccess: () => qc.invalidateQueries({ queryKey: keys.notifications }) });
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Notifications</h1>
          <p>Hand-offs, gentle reminders from your groups, and your own reminders.</p>
        </div>
        {data.some((n) => !n.readAt) && (
          <button type="button" className="btn" onClick={() => readAll.mutate()}>
            Mark all as read
          </button>
        )}
      </div>
      {isLoading ? (
        <SkeletonList />
      ) : data.length === 0 ? (
        <EmptyState title="All quiet" pose="sleepy" />
      ) : (
        <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
          {data.map((n) => (
            <li key={n.id} className={n.readAt ? "card-quiet row-between" : "card row-between"}>
              <span>
                {!n.readAt && <span className="visually-hidden">Unread: </span>}
                <strong style={{ fontWeight: n.readAt ? 500 : 700 }}>{n.title}</strong>
                <br />
                <span className="small muted">{new Date(n.createdAt).toLocaleString()}</span>
              </span>
              {!n.readAt && (
                <button type="button" className="btn btn-sm" onClick={() => read.mutate(n.id)}>
                  Mark read
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
