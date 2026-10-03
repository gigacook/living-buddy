import { useState } from "react";
import { Link } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { FolderKanban, House, Plus } from "lucide-react";
import type { GroupKind, GroupMode } from "@tendly/contracts";
import { api, errorMessage } from "../lib/api";
import { useGroups, useInvalidate, useMe, useMembers } from "../lib/queries";
import { Avatar, Dialog, EmptyState, Field, SkeletonList, Tip, useToast } from "../components/ui";

export const KIND_LABELS: Record<GroupKind, string> = {
  solo: "Just me",
  partners: "Partners",
  family: "Family",
  friends: "Friends",
  roommates: "Roommates",
  project_team: "Project team",
  custom: "Custom group",
};

export default function Groups() {
  const { data: groups = [], isLoading } = useGroups();
  const { data: members = [] } = useMembers();
  const me = useMe();
  const [creating, setCreating] = useState(false);
  const invalidate = useInvalidate();
  const toast = useToast();
  const join = useMutation({
    mutationFn: (id: string) => api.joinGroup(id),
    onSuccess: (g) => {
      invalidate("groups", "tasks");
      toast.show(`You joined “${g.name}”`);
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  const mine = groups.filter((g) => me && g.memberIds.includes(me.id) && !g.archived);
  const others = groups.filter((g) => me && !g.memberIds.includes(me.id) && !g.archived);
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Groups</h1>
          <p>Households share ongoing chores. Projects have goals, a board and a timeline.</p>
        </div>
        <button type="button" className="btn btn-primary" onClick={() => setCreating(true)}>
          <Plus size={18} aria-hidden /> New group
        </button>
      </div>
      <Tip id="groups-intro">
        <p className="small">Groups are for sharing responsibility, not keeping score. There are no rankings — just who's doing what, and a gentle reminder button if someone opts in.</p>
      </Tip>
      {isLoading ? (
        <SkeletonList />
      ) : mine.length === 0 ? (
        <EmptyState title="No groups yet" action={<button className="btn btn-primary" onClick={() => setCreating(true)}>Create one</button>}>
          <p>Make a household for shared chores, or a project for something with a finish line.</p>
        </EmptyState>
      ) : (
        <div className="grid-3">
          {mine.map((g) => (
            <Link key={g.id} to={`/groups/${g.id}`} className="card stack-sm" style={{ textDecoration: "none", color: "inherit" }}>
              <span className="row" style={{ fontWeight: 700, fontSize: "1.1rem" }}>
                {g.mode === "project" ? <FolderKanban size={20} aria-hidden /> : <House size={20} aria-hidden />}
                {g.name}
              </span>
              <span className="small muted">
                {KIND_LABELS[g.kind]} · {g.mode === "project" ? "Project" : "Ongoing household"}
                {g.goal ? ` · ${g.goal}` : ""}
              </span>
              <span className="row" aria-label={`${g.memberIds.length} people`}>
                {g.memberIds.map((id) => (
                  <Avatar key={id} member={members.find((m) => m.id === id)} />
                ))}
              </span>
            </Link>
          ))}
        </div>
      )}
      {others.length > 0 && (
        <section aria-labelledby="other-groups">
          <h2 id="other-groups" className="section-title">
            Other groups on this Tendly
          </h2>
          <ul className="stack-sm" style={{ listStyle: "none", padding: 0 }}>
            {others.map((g) => (
              <li key={g.id} className="row-between card-quiet">
                <span>
                  <strong>{g.name}</strong> <span className="muted small">{KIND_LABELS[g.kind]}</span>
                </span>
                <button type="button" className="btn btn-sm" onClick={() => join.mutate(g.id)}>
                  Join
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      <NewGroupDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}

function NewGroupDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { data: members = [] } = useMembers();
  const me = useMe();
  const [name, setName] = useState("");
  const [kind, setKind] = useState<GroupKind>("family");
  const [mode, setMode] = useState<GroupMode>("household");
  const [goal, setGoal] = useState("");
  const [start, setStart] = useState("");
  const [end, setEnd] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [error, setError] = useState<{ field?: string; message: string } | null>(null);
  const invalidate = useInvalidate();
  const create = useMutation({
    mutationFn: () => api.createGroup({ name, kind, mode, goal: goal || undefined, startDate: start || undefined, endDate: end || undefined, memberIds: [...new Set([me!.id, ...picked])] }),
    onSuccess: () => {
      invalidate("groups");
      setName("");
      setPicked([]);
      onClose();
    },
    onError: (e) => setError({ field: (e as { field?: string }).field, message: errorMessage(e) }),
  });
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="New group"
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" form="group-form" className="btn btn-primary" disabled={!name.trim() || create.isPending}>
            Create group
          </button>
        </>
      }
    >
      <form
        id="group-form"
        className="stack"
        onSubmit={(e) => {
          e.preventDefault();
          setError(null);
          create.mutate();
        }}
      >
        {error && !error.field && (
          <div className="banner banner-danger" role="alert">
            {error.message}
          </div>
        )}
        <Field label="Name" error={error?.field === "name" ? error.message : null}>
          {({ id, describedBy, invalid }) => <input id={id} className="input" value={name} onChange={(e) => setName(e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} autoFocus required />}
        </Field>
        <fieldset>
          <legend>What kind of group?</legend>
          <label className="check">
            <input type="radio" name="mode" checked={mode === "household"} onChange={() => { setMode("household"); if (kind === "project_team") setKind("family"); }} />
            Ongoing — shared upkeep, no finish line
          </label>
          <label className="check">
            <input type="radio" name="mode" checked={mode === "project"} onChange={() => { setMode("project"); setKind("project_team"); }} />
            Project — a goal, milestones and a board
          </label>
        </fieldset>
        <Field label="Who's in it?">
          {({ id }) => (
            <select id={id} className="select" value={kind} onChange={(e) => setKind(e.target.value as GroupKind)}>
              {Object.entries(KIND_LABELS).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          )}
        </Field>
        {mode === "project" && (
          <>
            <Field label="Goal">{({ id }) => <input id={id} className="input" value={goal} onChange={(e) => setGoal(e.target.value)} placeholder="Herbs growing by spring" />}</Field>
            <div className="form-grid">
              <Field label="Start">{({ id }) => <input id={id} type="date" className="input" value={start} onChange={(e) => setStart(e.target.value)} />}</Field>
              <Field label="Finish by" error={error?.field === "endDate" ? error.message : null}>
                {({ id, describedBy, invalid }) => <input id={id} type="date" className="input" value={end} onChange={(e) => setEnd(e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} />}
              </Field>
            </div>
          </>
        )}
        <fieldset>
          <legend>Add people</legend>
          <div className="chips" style={{ marginTop: 6 }}>
            {members
              .filter((m) => m.id !== me?.id)
              .map((m) => (
                <button key={m.id} type="button" className="chip" aria-pressed={picked.includes(m.id)} onClick={() => setPicked(picked.includes(m.id) ? picked.filter((x) => x !== m.id) : [...picked, m.id])}>
                  {m.displayName}
                </button>
              ))}
            {members.length <= 1 && <span className="small muted">Others appear here once they've opened Tendly and added their name.</span>}
          </div>
        </fieldset>
      </form>
    </Dialog>
  );
}
