import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, CalendarPlus, CheckSquare, ClipboardPaste, X } from "lucide-react";
import type { Suggestion } from "@tendly/contracts";
import { api, errorMessage } from "../lib/api";
import { localTimezone } from "../lib/dates";
import { keys, TASK_RELATED, useGroups, useInvalidate, useMe, useSession } from "../lib/queries";
import { CategoryBadge, Dialog, EmptyState, ErrorState, Field, Segmented, SkeletonList, Tip, useToast } from "../components/ui";
import { CategoryPicker, GroupSelect } from "../components/pickers";

const KIND: Record<Suggestion["draft"]["kind"], string> = { task: "Task", appointment: "Appointment", deadline: "Deadline", follow_up: "Follow-up" };

export default function Inbox() {
  const [status, setStatus] = useState<"pending" | "accepted" | "dismissed">("pending");
  const q = useQuery({ queryKey: keys.suggestions(status), queryFn: () => api.suggestions(status) });
  const connectors = useQuery({ queryKey: ["myConnectors"], queryFn: api.myConnectors });
  const [accepting, setAccepting] = useState<{ s: Suggestion; as: "task" | "event" } | null>(null);
  const qc = useQueryClient();
  const toast = useToast();
  const dismiss = useMutation({
    mutationFn: (id: string) => api.dismissSuggestion(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["suggestions"] });
      qc.invalidateQueries({ queryKey: keys.inboxCount });
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Inbox</h1>
          <p>Suggestions found in your messages. Nothing is added until you say so.</p>
        </div>
      </div>
      <Tip id="inbox-intro" pose="think">
        <p className="small">Tendly reads only the subject and a short preview, treats it as plain text, and never replies, pays, deletes or forwards anything. Fields marked “please check” were unclear.</p>
      </Tip>
      <PasteIntake />
      <Segmented
        label="Suggestion status"
        value={status}
        onChange={setStatus}
        options={[
          { value: "pending", label: "To review" },
          { value: "accepted", label: "Added" },
          { value: "dismissed", label: "Dismissed" },
        ]}
      />
      {q.error && <ErrorState message={errorMessage(q.error)} onRetry={() => q.refetch()} />}
      {q.isLoading ? (
        <SkeletonList />
      ) : (q.data ?? []).length === 0 ? (
        <EmptyState title={status === "pending" ? "Nothing to review" : "Nothing here"} pose="sleepy">
          {status === "pending" && <p>Paste an email above, or ask whoever runs Tendly to connect a mailbox.</p>}
        </EmptyState>
      ) : (
        <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
          {(q.data ?? []).map((s) => (
            <li key={s.id} className={`card stack-sm cat cat-${s.draft.category ?? "none"}`} style={{ borderLeft: "6px solid var(--c-line)" }}>
              <div className="row-between">
                <span className="row">
                  <span className="badge badge-plain">{KIND[s.draft.kind]}</span>
                  <CategoryBadge category={s.draft.category} />
                </span>
                <span className="small muted">
                  {s.connectorName} · {new Date(s.receivedAt).toLocaleDateString()}
                </span>
              </div>
              <strong style={{ fontSize: "1.05rem" }}>{s.draft.title}</strong>
              <span className="small muted">From: “{s.subject}”</span>
              <div className="row small">
                {s.draft.date && <span className={s.draft.uncertainFields.includes("date") ? "badge badge-warn" : "badge badge-plain"}>{s.draft.date}{s.draft.uncertainFields.includes("date") ? " · please check" : ""}</span>}
                {s.draft.time && <span className={s.draft.uncertainFields.includes("time") ? "badge badge-warn" : "badge badge-plain"}>{s.draft.time}{s.draft.uncertainFields.includes("time") ? " · please check" : ""}</span>}
                {!s.draft.date && <span className="badge badge-warn">No date found</span>}
                <span className="muted">Found by {s.extractor.startsWith("ai") ? "AI (your provider)" : "local rules"}</span>
              </div>
              {s.draft.evidence && <blockquote className="small" style={{ margin: 0, paddingLeft: 12, borderLeft: "3px solid var(--line)" }}>{s.draft.evidence}</blockquote>}
              {s.draft.flags.includes("possible_instructions_in_content") && (
                <div className="banner banner-warn small" role="note">
                  <AlertTriangle size={18} aria-hidden />
                  This message contained text that looked like instructions (for example to send passwords or pay). Tendly ignored it and treated it as plain text. Be careful with it.
                </div>
              )}
              {status === "pending" && (
                <div className="row">
                  <button type="button" className="btn btn-primary btn-sm" onClick={() => setAccepting({ s, as: "task" })}>
                    <CheckSquare size={16} aria-hidden /> Add as task
                  </button>
                  <button type="button" className="btn btn-sm" onClick={() => setAccepting({ s, as: "event" })}>
                    <CalendarPlus size={16} aria-hidden /> Add to calendar
                  </button>
                  <button type="button" className="btn btn-ghost btn-sm" onClick={() => dismiss.mutate(s.id)}>
                    <X size={16} aria-hidden /> Dismiss
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      {(connectors.data ?? []).length > 0 && (
        <section aria-labelledby="conn-heading">
          <h2 id="conn-heading" className="section-title">
            Your connected sources
          </h2>
          <ul className="stack-sm" style={{ listStyle: "none", padding: 0 }}>
            {(connectors.data ?? []).map((c) => (
              <li key={c.id} className="row-between card-quiet">
                <span>{c.displayName}</span>
                <span className="small muted">
                  {c.enabled ? c.status.replace("_", " ") : "off"}
                  {c.lastRunAt ? ` · checked ${new Date(c.lastRunAt).toLocaleString()}` : ""}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}
      {accepting && <AcceptDialog s={accepting.s} as={accepting.as} onClose={() => setAccepting(null)} />}
    </div>
  );
}

function PasteIntake() {
  const session = useSession();
  const [subject, setSubject] = useState("");
  const [text, setText] = useState("");
  const [useAi, setUseAi] = useState(false);
  const qc = useQueryClient();
  const toast = useToast();
  const intake = useMutation({
    mutationFn: () => api.intake({ subject: subject || undefined, text, useAi }),
    onSuccess: (r) => {
      qc.invalidateQueries({ queryKey: ["suggestions"] });
      qc.invalidateQueries({ queryKey: keys.inboxCount });
      toast.show(r.suggestions.length ? `Found ${r.suggestions.length} suggestion(s) to review.` : (r.notice ?? "Nothing found."));
      if (r.notice && r.suggestions.length) toast.show(r.notice);
      setText("");
      setSubject("");
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  return (
    <details className="disclosure card-quiet">
      <summary>
        <ClipboardPaste size={18} aria-hidden /> Paste a message
      </summary>
      <form
        className="stack"
        style={{ marginTop: 8 }}
        onSubmit={(e) => {
          e.preventDefault();
          intake.mutate();
        }}
      >
        <Field label="Subject (optional)">{({ id }) => <input id={id} className="input" value={subject} onChange={(e) => setSubject(e.target.value)} />}</Field>
        <Field label="Message text" hint="Paste only the relevant part. Quoted replies, signatures and link parameters are removed before anything is analyzed.">
          {({ id, describedBy }) => <textarea id={id} className="textarea" style={{ minHeight: 140 }} value={text} onChange={(e) => setText(e.target.value)} aria-describedby={describedBy} />}
        </Field>
        {session.data?.aiConfigured && (
          <label className="check">
            <input type="checkbox" checked={useAi} onChange={(e) => setUseAi(e.target.checked)} />
            Use the AI provider configured on this server (a shortened, redacted excerpt is sent to it)
          </label>
        )}
        <button type="submit" className="btn btn-primary" disabled={!text.trim() || intake.isPending}>
          {intake.isPending ? "Reading…" : "Find tasks and dates"}
        </button>
      </form>
    </details>
  );
}

function AcceptDialog({ s, as, onClose }: { s: Suggestion; as: "task" | "event"; onClose: () => void }) {
  const me = useMe();
  const { data: groups = [] } = useGroups();
  const [title, setTitle] = useState(s.draft.title);
  const [date, setDate] = useState(s.draft.date ?? "");
  const [time, setTime] = useState(s.draft.time ?? "");
  const [category, setCategory] = useState(s.draft.category);
  const [groupId, setGroupId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const invalidate = useInvalidate();
  const accept = useMutation({
    mutationFn: () =>
      api.acceptSuggestion(s.id, { createAs: as, title, date: date || undefined, time: time || undefined, category: category ?? undefined, groupId: groupId ?? undefined, timezone: localTimezone() }),
    onSuccess: () => {
      invalidate("suggestions", "inboxCount", ...TASK_RELATED);
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title={as === "task" ? "Add as a task" : "Add to your calendar"}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" form="accept-form" className="btn btn-primary" disabled={accept.isPending || !title.trim()}>
            Confirm and add
          </button>
        </>
      }
    >
      <form
        id="accept-form"
        className="stack"
        onSubmit={(e) => {
          e.preventDefault();
          setError(null);
          accept.mutate();
        }}
      >
        <p className="small muted">Check the details — they came from a message and may be wrong.</p>
        {error && (
          <div className="banner banner-danger" role="alert">
            {error}
          </div>
        )}
        <Field label="Title">{({ id }) => <input id={id} className="input" value={title} onChange={(e) => setTitle(e.target.value)} required />}</Field>
        <div className="form-grid">
          <Field label="Date" hint={s.draft.uncertainFields.includes("date") ? "Unclear in the message — please check." : undefined}>
            {({ id, describedBy }) => <input id={id} type="date" className="input" value={date} onChange={(e) => setDate(e.target.value)} aria-describedby={describedBy} required={as === "event"} />}
          </Field>
          <Field label="Time" hint={s.draft.uncertainFields.includes("time") ? "Unclear in the message — please check." : undefined}>
            {({ id, describedBy }) => <input id={id} type="time" className="input" value={time} onChange={(e) => setTime(e.target.value)} aria-describedby={describedBy} />}
          </Field>
        </div>
        <CategoryPicker value={category} onChange={setCategory} />
        {as === "task" && (
          <Field label="Add to" hint="Adding to a shared group makes it visible to everyone in it.">
            {({ id, describedBy }) => (
              <span aria-describedby={describedBy}>
                <GroupSelect id={id} groups={groups.filter((g) => me && g.memberIds.includes(me.id))} value={groupId} onChange={setGroupId} />
              </span>
            )}
          </Field>
        )}
      </form>
    </Dialog>
  );
}
