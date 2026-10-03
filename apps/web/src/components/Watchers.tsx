import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AlarmClock, BellRing } from "lucide-react";
import type { Alarm } from "@tendly/contracts";
import { api } from "../lib/api";
import { chime, showSystemNotification } from "../lib/notify";
import { keys } from "../lib/queries";
import { useTimer } from "../lib/useTimer";
import { phaseLabel } from "../lib/timer";
import { Dialog, useToast } from "./ui";

const NOTIFIED_KEY = "tendly.timer.notified";

function readNotified(): string | null {
  try {
    return localStorage.getItem(NOTIFIED_KEY);
  } catch {
    return null;
  }
}
function writeNotified(v: string) {
  try {
    localStorage.setItem(NOTIFIED_KEY, v);
  } catch {
    /* ignore */
  }
}

/** Announces the end of a focus/break phase exactly once per phase, on any page. */
export function TimerWatcher() {
  const { data } = useTimer();
  const toast = useToast();
  useEffect(() => {
    const ended = data?.state.phaseEndedAt;
    if (!data || !ended) return;
    if (data.state.status !== "finished" && data.state.status !== "phase_complete" && !(data.state.kind === "pomodoro" && data.state.status === "running")) return;
    if (readNotified() === ended) return;
    writeNotified(ended);
    // Don't alarm about phases that ended long ago (e.g. opening the app next morning).
    const ageMin = (Date.now() - new Date(ended).getTime()) / 60_000;
    const label = data.state.kind === "pomodoro" ? `${phaseLabel(data)} is next` : "Time's up";
    const text = data.state.label ? `${label} — ${data.state.label}` : label;
    if (ageMin < 15) {
      chime();
      showSystemNotification("Tendly timer", text);
    }
    toast.show(ageMin < 15 ? `⏰ ${text}` : `Your timer finished while you were away.`);
  }, [data, toast]);
  return null;
}

/** Shows alarm dialogs while the app is open. Missed alarms are shown once. */
export function AlarmWatcher() {
  const qc = useQueryClient();
  const { data: alarms = [] } = useQuery({ queryKey: keys.alarms, queryFn: api.alarms, refetchInterval: 60_000 });
  const [ringing, setRinging] = useState<Alarm | null>(null);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 15_000);
    const onVis = () => document.visibilityState === "visible" && setNow(Date.now());
    document.addEventListener("visibilitychange", onVis);
    return () => {
      window.clearInterval(id);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, []);
  useEffect(() => {
    if (ringing) return;
    const due = alarms.find((a) => a.enabled && a.nextFireAt && new Date(a.nextFireAt).getTime() <= now);
    if (due) {
      setRinging(due);
      if (due.sound) chime();
      showSystemNotification("Tendly alarm", due.label);
    }
  }, [alarms, now, ringing]);
  if (!ringing) return null;
  const missedMin = ringing.nextFireAt ? Math.round((Date.now() - new Date(ringing.nextFireAt).getTime()) / 60_000) : 0;
  const close = async (snooze?: number) => {
    const a = ringing;
    setRinging(null);
    if (snooze) await api.snoozeAlarm(a.id, snooze);
    else await api.alarmFired(a.id);
    qc.invalidateQueries({ queryKey: keys.alarms });
  };
  return (
    <Dialog
      open
      onClose={() => close()}
      title={ringing.label}
      footer={
        <>
          <button type="button" className="btn" onClick={() => close(10)}>
            <AlarmClock size={18} aria-hidden /> Snooze 10 min
          </button>
          <button type="button" className="btn btn-primary" onClick={() => close()} autoFocus>
            Got it
          </button>
        </>
      }
    >
      <div className="banner" role="alert">
        <BellRing size={22} aria-hidden />
        <span>
          {missedMin > 5 ? `Missed while Tendly was closed — it was set for ${ringing.time}.` : `It's ${ringing.time}.`}
        </span>
      </div>
    </Dialog>
  );
}
