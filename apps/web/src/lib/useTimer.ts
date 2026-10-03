import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { TimerView } from "@tendly/contracts";
import { api } from "./api";
import { keys } from "./queries";
import { clockOffset, remainingMs } from "./timer";

/**
 * Timer state from the server plus a locally ticking display value.
 * The interval only redraws; correctness comes from the server's absolute
 * deadline, re-fetched on focus, on visibility change (after sleep) and when
 * the local countdown reaches zero.
 */
export function useTimer() {
  const qc = useQueryClient();
  const offset = useRef(0);
  const query = useQuery({
    queryKey: keys.timer,
    queryFn: async () => {
      const v = await api.timer();
      offset.current = clockOffset(v);
      return v;
    },
    refetchInterval: 30_000,
    refetchOnWindowFocus: true,
  });
  const [now, setNow] = useState(() => Date.now());
  const running = query.data?.state.status === "running";
  useEffect(() => {
    if (!running) return;
    const id = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(id);
  }, [running]);
  useEffect(() => {
    const onVis = () => {
      if (document.visibilityState === "visible") qc.invalidateQueries({ queryKey: keys.timer });
    };
    document.addEventListener("visibilitychange", onVis);
    return () => document.removeEventListener("visibilitychange", onVis);
  }, [qc]);
  const remaining = query.data ? remainingMs(query.data, offset.current, now) : 0;
  useEffect(() => {
    if (running && remaining <= 0) qc.invalidateQueries({ queryKey: keys.timer });
  }, [running, remaining, qc]);
  const setView = (v: TimerView) => {
    offset.current = clockOffset(v);
    qc.setQueryData(keys.timer, v);
  };
  return { ...query, remaining, setView };
}
