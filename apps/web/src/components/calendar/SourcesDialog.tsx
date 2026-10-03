import { useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { FileUp, History, Link2, Plus, RefreshCw, Trash2 } from "lucide-react";
import type { CalendarSource } from "@tendly/contracts";
import { api, errorMessage } from "../../lib/api";
import { relativeFrom } from "../../lib/dates";
import { useGroups, useMe } from "../../lib/queries";
import { GroupSelect } from "../pickers";
import { Dialog, Field, Segmented, useToast } from "../ui";
import { useCalendarInvalidate, useSourcesQuery } from "./hooks";

type Mode = "list" | "file" | "url" | "local" | "history";

export function SourcesDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [mode, setMode] = useState<Mode>("list");
  const { data: sources = [], isLoading } = useSourcesQuery(open);
  const close = () => {
    setMode("list");
    onClose();
  };
  return (
    <Dialog open={open} onClose={close} title="Calendars" wide>
      <div className="stack">
        <Segmented
          label="Calendar actions"
          value={mode}
          onChange={setMode}
          options={[
            { value: "list", label: "Your calendars" },
            { value: "file", label: "Import file" },
            { value: "url", label: "Subscribe" },
            { value: "local", label: "New" },
            { value: "history", label: "Changes" },
          ]}
        />
        {mode === "list" && (isLoading ? <p className="muted">Loading…</p> : <SourceList sources={sources} />)}
        {mode === "file" && <ImportFile onDone={() => setMode("list")} sources={sources} />}
        {mode === "url" && <Subscribe onDone={() => setMode("list")} />}
        {mode === "local" && <NewLocal onDone={() => setMode("list")} />}
        {mode === "history" && <ChangeHistory sources={sources} />}
      </div>
    </Dialog>
  );
}

function SourceList({ sources }: { sources: CalendarSource[] }) {
  const invalidate = useCalendarInvalidate();
  const toast = useToast();
  const me = useMe();
  const refresh = useMutation({
    mutationFn: (id: string) => api.refreshSource(id),
    onSuccess: (r) => {
      invalidate();
      toast.show(`Refreshed: ${r.inserted} new, ${r.updated} changed, ${r.cancelled} removed.`);
    },
    onError: (e) => {
      invalidate();
      toast.show(errorMessage(e));
    },
  });
  const toggle = useMutation({ mutationFn: (s: CalendarSource) => api.updateSource(s.id, { enabled: !s.enabled }), onSuccess: invalidate });
  const remove = useMutation({
    mutationFn: (id: string) => api.deleteSource(id),
    onSuccess: () => {
      invalidate();
      toast.show("Calendar removed. Events from other calendars were not touched.");
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  if (sources.length === 0) return <p className="muted">No calendars yet. Import a file, subscribe to a link, or create one.</p>;
  return (
    <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
      {sources.map((s) => (
        <li key={s.id} className="card-quiet stack-sm">
          <div className="row-between">
            <strong>{s.name}</strong>
            <span className="row">
              <span className="badge badge-plain">{s.kind === "url" ? "Subscription" : s.kind === "file" ? "Imported" : "Tendly"}</span>
              {s.isPrivate ? <span className="badge badge-plain">Private</span> : <span className="badge badge-plain">Shared</span>}
              {s.lastStatus === "error" ? <span className="badge badge-danger">Problem</span> : s.lastStatus === "ok" ? <span className="badge badge-ok">OK</span> : null}
            </span>
          </div>
          <span className="small muted">
            {s.eventCount} events
            {s.urlDisplay ? ` · from ${s.urlDisplay}` : ""}
            {s.lastFetchedAt ? ` · updated ${relativeFrom(new Date(s.lastFetchedAt))}` : ""}
            {s.kind === "url" ? ` · refreshes about every ${s.refreshMinutes} min` : ""}
          </span>
          {s.lastError && (
            <span className="small" role="alert" style={{ color: "var(--danger)" }}>
              {s.lastError}
            </span>
          )}
          {s.warnings.length > 0 && (
            <details className="disclosure">
              <summary className="small">{s.warnings.length} note(s) from the last import</summary>
              <ul className="small">
                {s.warnings.map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
            </details>
          )}
          <div className="row">
            <label className="check">
              <input type="checkbox" checked={s.enabled} onChange={() => toggle.mutate(s)} />
              Show in Tendly
            </label>
            {s.kind === "url" && (
              <button type="button" className="btn btn-sm" onClick={() => refresh.mutate(s.id)} disabled={refresh.isPending}>
                <RefreshCw size={16} aria-hidden /> Refresh now
              </button>
            )}
            {(s.ownerId === me?.id || s.groupId) && (
              <button
                type="button"
                className="btn btn-sm btn-ghost btn-danger"
                onClick={() => {
                  if (window.confirm(`Remove “${s.name}” and its ${s.eventCount} events from Tendly?`)) remove.mutate(s.id);
                }}
              >
                <Trash2 size={16} aria-hidden /> Remove
              </button>
            )}
          </div>
        </li>
      ))}
    </ul>
  );
}

function SharingFields({ groupId, setGroupId, isPrivate, setPrivate }: { groupId: string | null; setGroupId: (g: string | null) => void; isPrivate: boolean; setPrivate: (b: boolean) => void }) {
  const { data: groups = [] } = useGroups();
  const me = useMe();
  return (
    <>
      <Field label="Belongs to">{({ id }) => <GroupSelect id={id} groups={groups.filter((g) => me && g.memberIds.includes(me.id))} value={groupId} onChange={setGroupId} noneLabel="Just me" />}</Field>
      {!groupId && (
        <label className="check">
          <input type="checkbox" checked={isPrivate} onChange={(e) => setPrivate(e.target.checked)} />
          Keep private (only I can see it)
        </label>
      )}
    </>
  );
}

function ImportFile({ onDone, sources }: { onDone: () => void; sources: CalendarSource[] }) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [target, setTarget] = useState("");
  const [groupId, setGroupId] = useState<string | null>(null);
  const [isPrivate, setPrivate] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const invalidate = useCalendarInvalidate();
  const toast = useToast();
  const imp = useMutation({
    mutationFn: async () => {
      const file = fileRef.current?.files?.[0];
      if (!file) throw new Error("Choose an .ics file first.");
      if (file.size > 5 * 1024 * 1024) throw new Error("That file is larger than 5 MB.");
      const text = await file.text();
      return api.importIcs(text, target ? { sourceId: target } : { name: file.name.replace(/\.ics$/i, ""), groupId: groupId ?? undefined, isPrivate: groupId ? false : isPrivate });
    },
    onSuccess: (r) => {
      invalidate();
      toast.show(`Imported “${r.source.name}”: ${r.inserted} new, ${r.updated} updated, ${r.unchanged} unchanged, ${r.cancelled} removed.`);
      onDone();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <form
      className="stack"
      onSubmit={(e) => {
        e.preventDefault();
        setError(null);
        imp.mutate();
      }}
    >
      <p className="small muted">Exported a calendar from Google, Apple, Outlook or another app? Import the .ics file here. Importing the same file again updates events instead of duplicating them.</p>
      <Field label="Calendar file (.ics)" error={error}>
        {({ id, describedBy }) => <input id={id} ref={fileRef} type="file" accept=".ics,text/calendar" className="input" style={{ paddingTop: 8 }} aria-describedby={describedBy} />}
      </Field>
      <Field label="Import into">
        {({ id }) => (
          <select id={id} className="select" value={target} onChange={(e) => setTarget(e.target.value)}>
            <option value="">A new calendar</option>
            {sources
              .filter((s) => s.kind === "file")
              .map((s) => (
                <option key={s.id} value={s.id}>
                  Update “{s.name}”
                </option>
              ))}
          </select>
        )}
      </Field>
      {!target && <SharingFields groupId={groupId} setGroupId={setGroupId} isPrivate={isPrivate} setPrivate={setPrivate} />}
      <button type="submit" className="btn btn-primary" disabled={imp.isPending}>
        <FileUp size={18} aria-hidden /> Import
      </button>
    </form>
  );
}

function Subscribe({ onDone }: { onDone: () => void }) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [groupId, setGroupId] = useState<string | null>(null);
  const [isPrivate, setPrivate] = useState(true);
  const [error, setError] = useState<{ field?: string; message: string } | null>(null);
  const invalidate = useCalendarInvalidate();
  const toast = useToast();
  const sub = useMutation({
    mutationFn: () => api.createSource({ name, kind: "url", url, groupId: groupId ?? undefined, isPrivate: groupId ? false : isPrivate }),
    onSuccess: (s) => {
      invalidate();
      toast.show(s.lastStatus === "ok" ? `Subscribed to “${s.name}” (${s.eventCount} events).` : `Added “${s.name}”, but the first refresh had a problem: ${s.lastError}`);
      onDone();
    },
    onError: (e) => setError({ field: (e as { field?: string }).field, message: errorMessage(e) }),
  });
  return (
    <form
      className="stack"
      onSubmit={(e) => {
        e.preventDefault();
        setError(null);
        sub.mutate();
      }}
    >
      <p className="small muted">
        Paste a calendar's “secret address in iCal format” or any https/webcal link. Tendly checks it about every hour. It's read-only: changes you make in the original app show up after the next refresh, and Tendly can't edit that calendar.
      </p>
      <Field label="Name" error={error?.field === "name" ? error.message : null}>
        {({ id, describedBy, invalid }) => <input id={id} className="input" value={name} onChange={(e) => setName(e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} required />}
      </Field>
      <Field label="Link" hint="Stored encrypted. Only the host name is shown afterwards." error={error?.field === "url" ? error.message : null}>
        {({ id, describedBy, invalid }) => <input id={id} className="input" type="url" inputMode="url" value={url} onChange={(e) => setUrl(e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} placeholder="webcal://…" required />}
      </Field>
      {error && !error.field && (
        <div className="banner banner-danger" role="alert">
          {error.message}
        </div>
      )}
      <SharingFields groupId={groupId} setGroupId={setGroupId} isPrivate={isPrivate} setPrivate={setPrivate} />
      <button type="submit" className="btn btn-primary" disabled={sub.isPending}>
        <Link2 size={18} aria-hidden /> {sub.isPending ? "Checking link…" : "Subscribe"}
      </button>
    </form>
  );
}

function NewLocal({ onDone }: { onDone: () => void }) {
  const [name, setName] = useState("");
  const [groupId, setGroupId] = useState<string | null>(null);
  const [isPrivate, setPrivate] = useState(true);
  const invalidate = useCalendarInvalidate();
  const toast = useToast();
  const create = useMutation({
    mutationFn: () => api.createSource({ name, kind: "local", groupId: groupId ?? undefined, isPrivate: groupId ? false : isPrivate }),
    onSuccess: () => {
      invalidate();
      onDone();
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  return (
    <form
      className="stack"
      onSubmit={(e) => {
        e.preventDefault();
        create.mutate();
      }}
    >
      <Field label="Name">{({ id }) => <input id={id} className="input" value={name} onChange={(e) => setName(e.target.value)} required placeholder="Family plans" />}</Field>
      <SharingFields groupId={groupId} setGroupId={setGroupId} isPrivate={isPrivate} setPrivate={setPrivate} />
      <button type="submit" className="btn btn-primary" disabled={!name.trim() || create.isPending}>
        <Plus size={18} aria-hidden /> Create calendar
      </button>
    </form>
  );
}

function ChangeHistory({ sources }: { sources: CalendarSource[] }) {
  const [sourceId, setSourceId] = useState("");
  const { data = [], isLoading } = useQuery({ queryKey: ["calendarChanges", sourceId], queryFn: () => api.calendarChanges(sourceId || undefined) });
  return (
    <div className="stack">
      <Field label="Calendar">
        {({ id }) => (
          <select id={id} className="select" value={sourceId} onChange={(e) => setSourceId(e.target.value)}>
            <option value="">All calendars and sharing</option>
            {sources.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        )}
      </Field>
      <p className="small muted">
        <History size={14} aria-hidden /> Subscribed feeds don't say who edited them, so their changes are shown as “external source / unknown actor”.
      </p>
      {isLoading ? (
        <p className="muted">Loading…</p>
      ) : data.length === 0 ? (
        <p className="muted">No changes recorded yet.</p>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th scope="col">When</th>
              <th scope="col">Who</th>
              <th scope="col">What</th>
              <th scope="col">Source</th>
              <th scope="col">Rev.</th>
            </tr>
          </thead>
          <tbody>
            {data.map((a) => (
              <tr key={a.id}>
                <td className="small">{new Date(a.at).toLocaleString()}</td>
                <td className="small">{a.actorName}</td>
                <td>
                  {a.summary} <span className="badge badge-plain">{a.op.replace("_", " ")}</span>
                </td>
                <td className="small muted">{a.source}</td>
                <td className="small">{a.revision || ""}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
