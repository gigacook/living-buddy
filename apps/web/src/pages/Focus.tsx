import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlarmClock, Bell, Hourglass, Pause, Play, Plus, RotateCcw, SkipForward, Trash2 } from "lucide-react";
import type { PomodoroConfig, TimerCommand, TimerKind } from "@tendly/contracts";
import { api, errorMessage } from "../lib/api";
import { countdownLabel, formatDuration, localTimezone, spokenDuration, todayIso } from "../lib/dates";
import { keys, useMe, useTasks } from "../lib/queries";
import { useTimer } from "../lib/useTimer";
import { phaseLabel, progress } from "../lib/timer";
import { permission, requestPermission, type PermissionState } from "../lib/notify";
import { setPrefs, usePrefs } from "../lib/prefs";
import { isNative } from "../lib/transport";
import { WEEKDAY_NAMES } from "../lib/recurrence";
import { Field, Segmented, Tip, useAnnouncer, useToast } from "../components/ui";
import { UsageCard } from "../components/UsageCard";

const PRESETS = [5, 15, 25, 45, 60];

export default function Focus() {
  const [params] = useSearchParams();
  const timer = useTimer();
  const toast = useToast();
  const me = useMe();
  const prefs = usePrefs();
  const [kind, setKind] = useState<TimerKind>("focus");
  const [minutes, setMinutes] = useState(Number(params.get("minutes")) || 25);
  const [taskId, setTaskId] = useState<string>(params.get("task") ?? "");
  const [label, setLabel] = useState("");
  const [config, setConfig] = useState<PomodoroConfig>({ focusMinutes: 25, shortBreakMinutes: 5, longBreakMinutes: 15, cyclesBeforeLongBreak: 4, autoStartBreaks: true, autoStartFocus: false });
  const [message, announce] = useAnnouncer();
  const { data: tasks = [] } = useTasks({ status: "open" });
  const v = timer.data;
  const status = v?.state.status ?? "idle";
  const active = status === "running" || status === "paused" || status === "phase_complete";

  const cmd = useMutation({
    mutationFn: (c: TimerCommand) => api.timerCommand(c, v?.version),
    onSuccess: (view, c) => {
      timer.setView(view);
      const spoken =
        c.action === "start"
          ? `${phaseLabel(view)} started, ${spokenDuration(view.remainingMs)}`
          : c.action === "pause"
            ? "Paused"
            : c.action === "resume"
              ? "Resumed"
              : c.action === "reset"
                ? "Timer reset"
                : c.action === "skip"
                  ? `${phaseLabel(view)} started`
                  : c.action === "continue"
                    ? `${phaseLabel(view)} started`
                    : "Added time";
      announce(spoken);
    },
    onError: (e) => {
      toast.show(errorMessage(e));
      timer.refetch();
    },
  });

  // Announce progress at most once per 5 minutes, never every second.
  const lastSpoken = useRef<number | null>(null);
  useEffect(() => {
    if (status !== "running") return;
    const mins = Math.ceil(timer.remaining / 60_000);
    if (mins > 0 && mins % 5 === 0 && lastSpoken.current !== mins) {
      lastSpoken.current = mins;
      if (mins !== Math.ceil((v?.state.durationMs ?? 0) / 60_000)) announce(`${mins} minutes left`);
    }
  }, [timer.remaining, status, v, announce]);

  const start = () =>
    cmd.mutate({
      action: "start",
      kind,
      minutes: kind === "pomodoro" ? null : minutes,
      label: label || tasks.find((t) => t.id === taskId)?.title || null,
      taskId: taskId || null,
      config: kind === "pomodoro" ? config : null,
    });

  const pct = v ? progress(v, timer.remaining) : 0;
  const R = 54;
  const C = 2 * Math.PI * R;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Focus</h1>
          <p>One thing at a time. The timer keeps going even if you reload or your laptop sleeps.</p>
        </div>
      </div>
      <div className="visually-hidden" aria-live="polite" role="status">
        {message}
      </div>

      <div className="grid-2">
        <section className="card" aria-labelledby="timer-heading">
          <h2 id="timer-heading" className="visually-hidden">
            Timer
          </h2>
          {!active && (
            <div className="stack">
              <Segmented
                label="Timer type"
                value={kind}
                onChange={setKind}
                options={[
                  { value: "focus", label: "Focus" },
                  { value: "pomodoro", label: "Pomodoro" },
                  { value: "countdown", label: "Countdown" },
                ]}
              />
              {kind !== "pomodoro" ? (
                <fieldset>
                  <legend>How long?</legend>
                  <div className="chips" style={{ marginTop: 6 }}>
                    {PRESETS.map((m) => (
                      <button key={m} type="button" className="chip" aria-pressed={minutes === m} onClick={() => setMinutes(m)}>
                        {m} min
                      </button>
                    ))}
                  </div>
                  <div className="field" style={{ marginTop: 8, maxWidth: 200 }}>
                    <label htmlFor="custom-minutes">Custom minutes</label>
                    <input id="custom-minutes" type="number" min={1} max={1440} className="input" value={minutes} onChange={(e) => setMinutes(Math.max(1, Number(e.target.value) || 1))} />
                  </div>
                </fieldset>
              ) : (
                <details className="disclosure" open>
                  <summary>Cycle settings</summary>
                  <div className="form-grid" style={{ marginTop: 8 }}>
                    {(
                      [
                        ["focusMinutes", "Focus (min)"],
                        ["shortBreakMinutes", "Short break (min)"],
                        ["longBreakMinutes", "Long break (min)"],
                        ["cyclesBeforeLongBreak", "Focus rounds before a long break"],
                      ] as const
                    ).map(([k, l]) => (
                      <Field key={k} label={l}>
                        {({ id }) => <input id={id} type="number" min={1} max={k === "cyclesBeforeLongBreak" ? 12 : 240} className="input" value={config[k]} onChange={(e) => setConfig({ ...config, [k]: Math.max(1, Number(e.target.value) || 1) })} />}
                      </Field>
                    ))}
                  </div>
                  <label className="check">
                    <input type="checkbox" checked={config.autoStartBreaks} onChange={(e) => setConfig({ ...config, autoStartBreaks: e.target.checked })} />
                    Start breaks automatically
                  </label>
                  <label className="check">
                    <input type="checkbox" checked={config.autoStartFocus} onChange={(e) => setConfig({ ...config, autoStartFocus: e.target.checked })} />
                    Start the next focus round automatically
                  </label>
                </details>
              )}
              <div className="form-grid">
                <Field label="Working on (optional)">
                  {({ id }) => (
                    <select id={id} className="select" value={taskId} onChange={(e) => setTaskId(e.target.value)}>
                      <option value="">Nothing specific</option>
                      {tasks.map((t) => (
                        <option key={t.id} value={t.id}>
                          {t.title}
                        </option>
                      ))}
                    </select>
                  )}
                </Field>
                <Field label="Label (optional)">
                  {({ id }) => <input id={id} className="input" value={label} onChange={(e) => setLabel(e.target.value)} maxLength={80} />}
                </Field>
              </div>
            </div>
          )}

          <div className="timer-wrap">
            <div className="timer-ring">
              <svg viewBox="0 0 120 120" aria-hidden>
                <circle className="track" cx="60" cy="60" r={R} fill="none" strokeWidth="10" />
                <circle className="progress" cx="60" cy="60" r={R} fill="none" strokeWidth="10" strokeLinecap="round" strokeDasharray={C} strokeDashoffset={C * (1 - pct)} />
              </svg>
              <div className="timer-readout">
                <div className="timer-digits" role="timer" aria-label={`Time remaining: ${spokenDuration(active ? timer.remaining : minutes * 60_000)}`}>
                  {active ? formatDuration(timer.remaining) : formatDuration((kind === "pomodoro" ? config.focusMinutes : minutes) * 60_000)}
                </div>
                <div className="timer-phase">
                  {v && active ? (status === "paused" ? "Paused" : status === "phase_complete" ? `Next: ${phaseLabel(v)}` : phaseLabel(v)) : status === "finished" ? "Done — nice work" : "Ready"}
                </div>
                {v?.state.label && active && <div className="small muted">{v.state.label}</div>}
              </div>
            </div>
            {v?.state.kind === "pomodoro" && active && <p className="muted small">Focus rounds completed: {v.state.completedFocus}</p>}
            {status === "phase_complete" && v && (
              <div className="banner" role="status">
                {v.state.phaseEndedAt && Date.now() - new Date(v.state.phaseEndedAt).getTime() > 120_000 ? "That part ended while you were away. " : "Time for the next part. "}
                Start when you're ready.
              </div>
            )}
            <div className="row" style={{ justifyContent: "center" }}>
              {!active && (
                <button type="button" className="btn btn-primary btn-lg" onClick={start} disabled={cmd.isPending}>
                  <Play size={20} aria-hidden /> Start
                </button>
              )}
              {status === "running" && (
                <button type="button" className="btn btn-primary btn-lg" onClick={() => cmd.mutate({ action: "pause" })}>
                  <Pause size={20} aria-hidden /> Pause
                </button>
              )}
              {status === "paused" && (
                <button type="button" className="btn btn-primary btn-lg" onClick={() => cmd.mutate({ action: "resume" })}>
                  <Play size={20} aria-hidden /> Resume
                </button>
              )}
              {status === "phase_complete" && (
                <button type="button" className="btn btn-primary btn-lg" onClick={() => cmd.mutate({ action: "continue" })}>
                  <Play size={20} aria-hidden /> Start {v ? phaseLabel(v).toLowerCase() : ""}
                </button>
              )}
              {active && v?.state.kind === "pomodoro" && (
                <button type="button" className="btn btn-lg" onClick={() => cmd.mutate({ action: "skip" })}>
                  <SkipForward size={20} aria-hidden /> {v.state.phase === "focus" && status !== "phase_complete" ? "Skip to break" : "Skip break"}
                </button>
              )}
              {(status === "running" || status === "paused") && v?.state.kind !== "pomodoro" && (
                <button type="button" className="btn btn-lg" onClick={() => cmd.mutate({ action: "extend", minutes: 5 })}>
                  +5 min
                </button>
              )}
              {(active || status === "finished") && (
                <button type="button" className="btn btn-ghost btn-lg" onClick={() => cmd.mutate({ action: "reset" })}>
                  <RotateCcw size={20} aria-hidden /> Reset
                </button>
              )}
            </div>
          </div>
        </section>

        <div className="stack">
          <NotificationSettings />
          <Alarms />
          <Countdowns />
          {me?.prefs.showUsageCard && <UsageCard />}
          <label className="check">
            <input type="checkbox" checked={prefs.sound} onChange={(e) => setPrefs({ sound: e.target.checked })} />
            Play a soft chime when time is up
          </label>
        </div>
      </div>
    </div>
  );
}

function NotificationSettings() {
  const [perm, setPerm] = useState<PermissionState>("default");
  useEffect(() => {
    permission().then(setPerm);
  }, []);
  return (
    <section className="card stack-sm" aria-labelledby="notify-heading">
      <h2 id="notify-heading" className="row">
        <Bell size={20} aria-hidden /> Reminders on this device
      </h2>
      {perm === "granted" ? (
        <p className="small muted">System notifications are on. You'll also always see a message inside Tendly.</p>
      ) : perm === "denied" ? (
        <p className="small muted">Notifications are blocked in your browser or system settings. Tendly will still show reminders on screen while it's open.</p>
      ) : perm === "unsupported" ? (
        <p className="small muted">This browser doesn't support notifications. Reminders appear on screen while Tendly is open.</p>
      ) : (
        <>
          <p className="small muted">Allow notifications so timers and alarms can reach you while Tendly is in the background.</p>
          <button type="button" className="btn btn-soft" onClick={() => requestPermission().then(setPerm)}>
            Allow notifications
          </button>
        </>
      )}
      <Tip id="alarm-limits" pose="think">
        <p className="small">
          {isNative()
            ? "Alarms ring while the Tendly app is running. Your phone or computer may pause apps in the background, so don't rely on Tendly as your only wake-up alarm."
            : "Alarms ring while a Tendly tab is open. A closed browser can't ring, and background tabs may be delayed — keep a tab open or use the desktop app for important alarms."}
        </p>
      </Tip>
    </section>
  );
}

function Alarms() {
  const qc = useQueryClient();
  const toast = useToast();
  const { data: alarms = [] } = useQuery({ queryKey: keys.alarms, queryFn: api.alarms });
  const [label, setLabel] = useState("");
  const [time, setTime] = useState("08:00");
  const [days, setDays] = useState<number>(0);
  const create = useMutation({
    mutationFn: () => api.createAlarm({ label: label || "Alarm", time, weekdays: days, timezone: localTimezone(), sound: true }),
    onSuccess: () => {
      setLabel("");
      qc.invalidateQueries({ queryKey: keys.alarms });
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  const toggle = useMutation({
    mutationFn: (a: (typeof alarms)[number]) => api.updateAlarm(a.id, { label: a.label, time: a.time, weekdays: a.weekdays, date: a.date ?? undefined, timezone: a.timezone, enabled: !a.enabled, sound: a.sound }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.alarms }),
  });
  const remove = useMutation({ mutationFn: (id: string) => api.deleteAlarm(id), onSuccess: () => qc.invalidateQueries({ queryKey: keys.alarms }) });
  const dayLabel = (mask: number) => (mask === 0 ? "Every day" : WEEKDAY_NAMES.filter((_, i) => mask & (1 << i)).join(", "));
  return (
    <section className="card stack-sm" aria-labelledby="alarms-heading">
      <h2 id="alarms-heading" className="row">
        <AlarmClock size={20} aria-hidden /> Alarms
      </h2>
      {alarms.length === 0 && <p className="small muted">No alarms yet.</p>}
      <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {alarms.map((a) => (
          <li key={a.id} className="row-between">
            <span>
              <strong style={{ fontVariantNumeric: "tabular-nums" }}>{a.time}</strong> {a.label}
              <br />
              <span className="small muted">{a.date ?? dayLabel(a.weekdays)}</span>
            </span>
            <span className="row">
              <label className="check">
                <input type="checkbox" checked={a.enabled} onChange={() => toggle.mutate(a)} />
                <span className="visually-hidden">Alarm {a.label} at {a.time} enabled</span>
                On
              </label>
              <button type="button" className="btn btn-ghost btn-icon" aria-label={`Delete alarm ${a.label}`} onClick={() => remove.mutate(a.id)}>
                <Trash2 size={18} aria-hidden />
              </button>
            </span>
          </li>
        ))}
      </ul>
      <details className="disclosure">
        <summary>Add an alarm</summary>
        <form
          className="stack-sm"
          onSubmit={(e) => {
            e.preventDefault();
            create.mutate();
          }}
        >
          <div className="form-grid">
            <Field label="Label">{({ id }) => <input id={id} className="input" value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Take meds" maxLength={80} />}</Field>
            <Field label="Time">{({ id }) => <input id={id} type="time" className="input" value={time} onChange={(e) => setTime(e.target.value)} required />}</Field>
          </div>
          <fieldset>
            <legend className="small">Days (none selected = every day)</legend>
            <div className="chips" style={{ marginTop: 6 }}>
              {WEEKDAY_NAMES.map((n, i) => (
                <button key={n} type="button" className="chip" aria-pressed={!!(days & (1 << i))} onClick={() => setDays(days ^ (1 << i))}>
                  {n}
                </button>
              ))}
            </div>
          </fieldset>
          <button type="submit" className="btn btn-primary" disabled={create.isPending}>
            <Plus size={18} aria-hidden /> Add alarm
          </button>
        </form>
      </details>
    </section>
  );
}

function Countdowns() {
  const qc = useQueryClient();
  const toast = useToast();
  const { data: list = [] } = useQuery({ queryKey: keys.countdowns, queryFn: api.countdowns });
  const [title, setTitle] = useState("");
  const [date, setDate] = useState(todayIso());
  const create = useMutation({
    mutationFn: () => api.createCountdown({ title, date, timezone: localTimezone() }),
    onSuccess: () => {
      setTitle("");
      qc.invalidateQueries({ queryKey: keys.countdowns });
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  const remove = useMutation({ mutationFn: (id: string) => api.deleteCountdown(id), onSuccess: () => qc.invalidateQueries({ queryKey: keys.countdowns }) });
  return (
    <section className="card stack-sm" aria-labelledby="cd-heading">
      <h2 id="cd-heading" className="row">
        <Hourglass size={20} aria-hidden /> Countdowns
      </h2>
      <ul className="stack-sm" style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {list.map((c) => (
          <li key={c.id} className="row-between">
            <span>
              {c.title} <span className="small muted">({c.date})</span>
            </span>
            <span className="row">
              <strong>{countdownLabel(new Date(c.targetAt))}</strong>
              <button type="button" className="btn btn-ghost btn-icon" aria-label={`Delete countdown ${c.title}`} onClick={() => remove.mutate(c.id)}>
                <Trash2 size={18} aria-hidden />
              </button>
            </span>
          </li>
        ))}
      </ul>
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          if (title.trim()) create.mutate();
        }}
      >
        <label htmlFor="cd-title" className="visually-hidden">
          Countdown name
        </label>
        <input id="cd-title" className="input" style={{ flex: 2, minWidth: 140 }} placeholder="Exam, trip, deadline…" value={title} onChange={(e) => setTitle(e.target.value)} />
        <label htmlFor="cd-date" className="visually-hidden">
          Date
        </label>
        <input id="cd-date" type="date" className="input" style={{ flex: 1, minWidth: 140 }} value={date} onChange={(e) => setDate(e.target.value)} />
        <button type="submit" className="btn" disabled={!title.trim()}>
          Add
        </button>
      </form>
    </section>
  );
}
