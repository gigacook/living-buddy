import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Gauge, Pencil } from "lucide-react";
import type { UsageView, WindowSummary } from "@tendly/contracts";
import { api, errorMessage } from "../lib/api";
import { keys } from "../lib/queries";
import { relativeFrom } from "../lib/dates";
import { Dialog, Field } from "./ui";

function toLocalInput(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  const off = d.getTimezoneOffset();
  return new Date(d.getTime() - off * 60_000).toISOString().slice(0, 16);
}

function WindowMeter({ title, w, checkpoints }: { title: string; w: WindowSummary; checkpoints: number[] }) {
  const pct = w.state === "current" ? (w.percent ?? 0) : 0;
  return (
    <div className="stack-sm">
      <div className="row-between">
        <strong>{title}</strong>
        <span className="muted small">
          {w.state === "unknown" ? "Not entered" : w.state === "reset_passed" ? "Window has reset — update when you check" : `About ${Math.round(pct)}%`}
        </span>
      </div>
      <div className="meter" role="img" aria-label={`${title}: ${w.state === "current" ? `about ${Math.round(pct)} percent used` : "no current value"}`}>
        <span style={{ width: `${pct}%` }} />
      </div>
      <div className="meter-marks" aria-hidden>
        <span>0</span>
        {checkpoints.map((c) => (
          <span key={c} style={{ fontWeight: w.reached.includes(c) ? 700 : 400 }}>
            {c}%{w.reached.includes(c) ? " ✓" : ""}
          </span>
        ))}
      </div>
      {w.resetsAt && <span className="small muted">Resets {relativeFrom(new Date(w.resetsAt))} ({new Date(w.resetsAt).toLocaleString()})</span>}
    </div>
  );
}

/** Optional, unobtrusive tracker for Claude's own usage limits. Manual entry only. */
export function UsageCard({ compact }: { compact?: boolean }) {
  const { data, isLoading } = useQuery({ queryKey: keys.usage, queryFn: api.usage });
  const [open, setOpen] = useState(false);
  if (isLoading || !data) return null;
  return (
    <section className="card stack" aria-labelledby="usage-title">
      <div className="row-between">
        <h2 id="usage-title" className="row" style={{ margin: 0 }}>
          <Gauge size={20} aria-hidden /> Claude usage
        </h2>
        <button type="button" className="btn btn-sm" onClick={() => setOpen(true)}>
          <Pencil size={16} aria-hidden /> Update
        </button>
      </div>
      <p className="small muted" style={{ margin: 0 }}>
        Estimate you entered by hand{data.updatedAt ? ` · updated ${relativeFrom(new Date(data.updatedAt))}` : ""}. Tendly doesn't read your Claude account.
      </p>
      <WindowMeter title="5-hour window" w={data.fiveHour} checkpoints={[25, 50, 75, 100]} />
      <WindowMeter title="7-day window" w={data.sevenDay} checkpoints={[50, 100]} />
      {!compact && data.notes && <p className="small">{data.notes}</p>}
      <UsageDialog open={open} onClose={() => setOpen(false)} data={data} />
    </section>
  );
}

function UsageDialog({ open, onClose, data }: { open: boolean; onClose: () => void; data: UsageView }) {
  const qc = useQueryClient();
  const [five, setFive] = useState(data.fiveHour.percent?.toString() ?? "");
  const [fiveReset, setFiveReset] = useState(toLocalInput(data.fiveHour.resetsAt));
  const [seven, setSeven] = useState(data.sevenDay.percent?.toString() ?? "");
  const [sevenReset, setSevenReset] = useState(toLocalInput(data.sevenDay.resetsAt));
  const [notes, setNotes] = useState(data.notes ?? "");
  const [thresholds, setThresholds] = useState<number[]>(data.reminderThresholds);
  const [error, setError] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: () =>
      api.putUsage({
        fiveHourPercent: five === "" ? null : Number(five),
        fiveHourResetsAt: fiveReset ? new Date(fiveReset).toISOString() : null,
        sevenDayPercent: seven === "" ? null : Number(seven),
        sevenDayResetsAt: sevenReset ? new Date(sevenReset).toISOString() : null,
        notes: notes || null,
        reminderThresholds: thresholds,
        remindOnReset: data.remindOnReset,
      }),
    onSuccess: (v) => {
      qc.setQueryData(keys.usage, v);
      qc.invalidateQueries({ queryKey: keys.notifications });
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Update Claude usage"
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" form="usage-form" className="btn btn-primary" disabled={save.isPending}>
            Save
          </button>
        </>
      }
    >
      <form
        id="usage-form"
        className="stack"
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate();
        }}
      >
        <p className="small muted">Copy the numbers from Claude's usage page. There is no official API for these limits, so Tendly keeps them as your own notes.</p>
        {error && (
          <div className="banner banner-danger" role="alert">
            {error}
          </div>
        )}
        <div className="form-grid">
          <Field label="5-hour window used (%)">
            {({ id }) => <input id={id} type="number" min={0} max={100} className="input" value={five} onChange={(e) => setFive(e.target.value)} />}
          </Field>
          <Field label="5-hour window resets at">
            {({ id }) => <input id={id} type="datetime-local" className="input" value={fiveReset} onChange={(e) => setFiveReset(e.target.value)} />}
          </Field>
          <Field label="7-day window used (%)">
            {({ id }) => <input id={id} type="number" min={0} max={100} className="input" value={seven} onChange={(e) => setSeven(e.target.value)} />}
          </Field>
          <Field label="7-day window resets at">
            {({ id }) => <input id={id} type="datetime-local" className="input" value={sevenReset} onChange={(e) => setSevenReset(e.target.value)} />}
          </Field>
        </div>
        <fieldset>
          <legend>Remind me when my entry passes</legend>
          <div className="chips" style={{ marginTop: 6 }}>
            {[25, 50, 75, 90, 100].map((t) => (
              <button key={t} type="button" className="chip" aria-pressed={thresholds.includes(t)} onClick={() => setThresholds(thresholds.includes(t) ? thresholds.filter((x) => x !== t) : [...thresholds, t])}>
                {t}%
              </button>
            ))}
          </div>
        </fieldset>
        <Field label="Notes">
          {({ id }) => <textarea id={id} className="textarea" value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={2000} />}
        </Field>
      </form>
    </Dialog>
  );
}
