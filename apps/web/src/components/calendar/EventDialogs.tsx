import { useEffect, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { Category, EventOccurrence } from "@tendly/contracts";
import { api, errorMessage } from "../../lib/api";
import { longDate, timeOf } from "../../lib/dates";
import { buildRule, type RepeatPreset } from "../../lib/recurrence";
import { CategoryPicker, RepeatPicker } from "../pickers";
import { CategoryBadge, Dialog, Field, useToast } from "../ui";
import { TaskEditor } from "../TaskEditor";
import { useCalendarInvalidate, useSourcesQuery } from "./hooks";

export function NewEventDialog({ date, onClose, timezone }: { date: string | null; onClose: () => void; timezone: string }) {
  const { data: sources = [] } = useSourcesQuery(!!date);
  const local = sources.filter((s) => s.kind === "local");
  const [sourceId, setSourceId] = useState("");
  const [title, setTitle] = useState("");
  const [allDay, setAllDay] = useState(false);
  const [start, setStart] = useState("");
  const [startTime, setStartTime] = useState("09:00");
  const [endTime, setEndTime] = useState("10:00");
  const [location, setLocation] = useState("");
  const [category, setCategory] = useState<Category | null>(null);
  const [repeat, setRepeat] = useState<{ preset: RepeatPreset; days: string[]; custom: string }>({ preset: "none", days: [], custom: "" });
  const [error, setError] = useState<string | null>(null);
  const invalidate = useCalendarInvalidate();
  const toast = useToast();
  useEffect(() => {
    if (date) {
      setStart(date);
      setTitle("");
      setError(null);
    }
  }, [date]);
  useEffect(() => {
    if (!sourceId && local[0]) setSourceId(local[0].id);
  }, [local, sourceId]);
  const create = useMutation({
    mutationFn: async () => {
      let sid = sourceId;
      if (!sid) sid = (await api.createSource({ name: "My calendar", kind: "local" })).id;
      return api.createEvent({
        sourceId: sid,
        title,
        allDay,
        startDate: start,
        startTime: allDay ? undefined : startTime,
        endTime: allDay ? undefined : endTime,
        location: location || undefined,
        timezone,
        category: category ?? undefined,
        recurrence: buildRule(repeat.preset, repeat.days, repeat.custom) ?? undefined,
      });
    },
    onSuccess: (e) => {
      invalidate();
      toast.show(`Added “${e.title}”`);
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <Dialog
      open={!!date}
      onClose={onClose}
      title="New event"
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" form="event-form" className="btn btn-primary" disabled={create.isPending || !title.trim()}>
            Add event
          </button>
        </>
      }
    >
      <form
        id="event-form"
        className="stack"
        onSubmit={(e) => {
          e.preventDefault();
          setError(null);
          create.mutate();
        }}
      >
        {error && (
          <div className="banner banner-danger" role="alert">
            {error}
          </div>
        )}
        <Field label="Title">{({ id }) => <input id={id} className="input" value={title} onChange={(e) => setTitle(e.target.value)} required autoFocus />}</Field>
        <div className="form-grid">
          <Field label="Date">{({ id }) => <input id={id} type="date" className="input" value={start} onChange={(e) => setStart(e.target.value)} required />}</Field>
          {!allDay && <Field label="Starts">{({ id }) => <input id={id} type="time" className="input" value={startTime} onChange={(e) => setStartTime(e.target.value)} />}</Field>}
          {!allDay && <Field label="Ends">{({ id }) => <input id={id} type="time" className="input" value={endTime} onChange={(e) => setEndTime(e.target.value)} />}</Field>}
        </div>
        <label className="check">
          <input type="checkbox" checked={allDay} onChange={(e) => setAllDay(e.target.checked)} />
          All day
        </label>
        <RepeatPicker {...repeat} onChange={setRepeat} />
        <CategoryPicker value={category} onChange={setCategory} />
        <Field label="Place (optional)">{({ id }) => <input id={id} className="input" value={location} onChange={(e) => setLocation(e.target.value)} />}</Field>
        <Field label="Calendar" hint={local.length === 0 ? "A personal calendar called “My calendar” will be created." : undefined}>
          {({ id, describedBy }) => (
            <select id={id} className="select" value={sourceId} onChange={(e) => setSourceId(e.target.value)} aria-describedby={describedBy}>
              {local.length === 0 && <option value="">My calendar (new)</option>}
              {local.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          )}
        </Field>
      </form>
    </Dialog>
  );
}

export function EventDialog({ occurrence, onClose }: { occurrence: EventOccurrence | null; onClose: () => void }) {
  const isTask = occurrence?.kind === "task";
  const { data: task } = useQuery({ queryKey: ["task", occurrence?.eventId], queryFn: () => api.task(occurrence!.eventId), enabled: !!occurrence && isTask });
  const { data: ev } = useQuery({ queryKey: ["event", occurrence?.eventId], queryFn: () => api.event(occurrence!.eventId), enabled: !!occurrence && !isTask && !!occurrence.eventId });
  const [note, setNote] = useState("");
  const [category, setCategory] = useState<Category | null>(null);
  const invalidate = useCalendarInvalidate();
  const toast = useToast();
  useEffect(() => {
    setNote(ev?.localOverride?.note ?? "");
    setCategory(ev?.localOverride?.category ?? ev?.category ?? null);
  }, [ev]);
  const save = useMutation({
    mutationFn: (p: { hidden?: boolean; cancel?: boolean }) =>
      api.updateEvent(ev!.id, p.cancel ? { expectedRevision: ev!.revision, cancel: true } : { expectedRevision: ev!.revision, localOverride: { category: category ?? undefined, note: note || undefined, hidden: p.hidden } }),
    onSuccess: (_, p) => {
      invalidate();
      toast.show(p.cancel ? "Event cancelled." : p.hidden ? "Hidden in Tendly." : "Saved in Tendly.");
      onClose();
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  if (isTask) return <TaskEditor open={!!occurrence && !!task} task={task} onClose={onClose} />;
  if (!occurrence) return null;
  const o = occurrence;
  return (
    <Dialog open={!!occurrence} onClose={onClose} title={o.title}>
      <div className="stack">
        <p>
          {o.allDay ? `${longDate(o.startDate!)}${o.endDate && o.endDate !== o.startDate ? ` – ${longDate(o.endDate)}` : ""} · All day` : `${longDate(o.startDate ?? new Date(o.start).toISOString().slice(0, 10))} · ${timeOf(new Date(o.start))}–${timeOf(new Date(o.end))}`}
        </p>
        <div className="row">
          <CategoryBadge category={o.category} />
          {o.sourceName && <span className="badge badge-plain">{o.sourceName}</span>}
          {o.recurring && <span className="badge badge-plain">Repeats</span>}
          {o.alsoIn.length > 0 && <span className="badge badge-plain">Also in {o.alsoIn.length} other calendar(s)</span>}
        </div>
        {o.location && <p>📍 {o.location}</p>}
        {o.description && <p style={{ whiteSpace: "pre-wrap" }}>{o.description}</p>}
        {ev && !ev.editable && (
          <div className="card-quiet stack">
            <p className="small muted" style={{ margin: 0 }}>
              This comes from a calendar Tendly reads but can't edit. Adjust how it shows here:
            </p>
            <CategoryPicker value={category} onChange={setCategory} label="Category in Tendly" />
            <Field label="Private note">{({ id }) => <textarea id={id} className="textarea" value={note} onChange={(e) => setNote(e.target.value)} maxLength={1000} />}</Field>
            <div className="row">
              <button type="button" className="btn btn-primary" onClick={() => save.mutate({})}>
                Save
              </button>
              <button type="button" className="btn" onClick={() => save.mutate({ hidden: true })}>
                Hide in Tendly
              </button>
            </div>
          </div>
        )}
        {ev?.editable && (
          <div className="row">
            <button type="button" className="btn btn-danger" onClick={() => save.mutate({ cancel: true })}>
              Cancel event
            </button>
          </div>
        )}
      </div>
    </Dialog>
  );
}
