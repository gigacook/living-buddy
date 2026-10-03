/**
 * Per-device UI preferences. Stored in localStorage because they are harmless
 * conveniences (theme, which tips were dismissed, the chosen display name's id).
 * Secrets such as API keys are never stored here.
 */
import { useSyncExternalStore } from "react";

export type UiPrefs = {
  theme: "system" | "light" | "dark";
  motion: "system" | "reduced";
  quiet: boolean; // hides mascot tips and mutes sounds
  sound: boolean;
  dismissedTips: string[];
  calendarView: "month" | "week" | "agenda";
};

const DEFAULTS: UiPrefs = { theme: "system", motion: "system", quiet: false, sound: true, dismissedTips: [], calendarView: "month" };
const KEY = "tendly.ui";
const ACTOR = "tendly.actor";

function safeGet(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeSet(key: string, value: string | null) {
  try {
    if (value === null) window.localStorage.removeItem(key);
    else window.localStorage.setItem(key, value);
  } catch {
    /* private mode or storage disabled: keep working without persistence */
  }
}

let current: UiPrefs = load();
const listeners = new Set<() => void>();

function load(): UiPrefs {
  try {
    return { ...DEFAULTS, ...(JSON.parse(safeGet(KEY) ?? "{}") as Partial<UiPrefs>) };
  } catch {
    return { ...DEFAULTS };
  }
}

export function getPrefs(): UiPrefs {
  return current;
}

export function setPrefs(p: Partial<UiPrefs>) {
  current = { ...current, ...p };
  safeSet(KEY, JSON.stringify(current));
  applyDocumentPrefs();
  listeners.forEach((l) => l());
}

export function usePrefs(): UiPrefs {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => current,
    () => current,
  );
}

export function applyDocumentPrefs() {
  const root = document.documentElement;
  if (current.theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", current.theme);
  if (current.motion === "reduced") root.setAttribute("data-motion", "reduced");
  else root.removeAttribute("data-motion");
}

export function prefersReducedMotion(): boolean {
  if (current.motion === "reduced") return true;
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}

let actorCache: string | null | undefined;
const actorListeners = new Set<() => void>();

export function getActorId(): string | null {
  if (actorCache === undefined) actorCache = safeGet(ACTOR);
  return actorCache;
}

export function setActorId(id: string | null) {
  actorCache = id;
  safeSet(ACTOR, id);
  actorListeners.forEach((l) => l());
}

export function useActorId(): string | null {
  return useSyncExternalStore(
    (cb) => {
      actorListeners.add(cb);
      return () => actorListeners.delete(cb);
    },
    getActorId,
    getActorId,
  );
}

export function dismissTip(id: string) {
  if (!current.dismissedTips.includes(id)) setPrefs({ dismissedTips: [...current.dismissedTips, id] });
}
