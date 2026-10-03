/**
 * Notifications and sounds.
 *
 * Honest limits: in a browser, reminders can only appear while a Tendly tab is
 * open (background tabs may be delayed by the browser). The native app uses
 * the operating system's notification service while it is running. Neither
 * guarantees an alarm when the app is fully closed or the device restricts
 * background activity. A visual banner is always shown as well, so sound and
 * system notifications are never the only signal.
 */
import { isNative } from "./transport";
import { getPrefs } from "./prefs";

export type PermissionState = "granted" | "denied" | "default" | "unsupported";

export async function permission(): Promise<PermissionState> {
  if (isNative()) {
    try {
      const n = await import("@tauri-apps/plugin-notification");
      return (await n.isPermissionGranted()) ? "granted" : "default";
    } catch {
      return "unsupported";
    }
  }
  if (typeof Notification === "undefined") return "unsupported";
  return Notification.permission as PermissionState;
}

export async function requestPermission(): Promise<PermissionState> {
  if (isNative()) {
    try {
      const n = await import("@tauri-apps/plugin-notification");
      const r = await n.requestPermission();
      return r === "granted" ? "granted" : "denied";
    } catch {
      return "unsupported";
    }
  }
  if (typeof Notification === "undefined") return "unsupported";
  return (await Notification.requestPermission()) as PermissionState;
}

export async function showSystemNotification(title: string, body?: string) {
  if ((await permission()) !== "granted") return;
  try {
    if (isNative()) {
      const n = await import("@tauri-apps/plugin-notification");
      n.sendNotification({ title, body });
      return;
    }
    new Notification(title, { body, tag: `tendly-${title}`, silent: true });
  } catch {
    /* notifications are best-effort */
  }
}

let ctx: AudioContext | null = null;

/** A soft two-note chime synthesized locally (no audio files, no network). */
export function chime() {
  const prefs = getPrefs();
  if (!prefs.sound || prefs.quiet) return;
  try {
    const AC = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AC) return;
    ctx = ctx ?? new AC();
    const now = ctx.currentTime;
    [659.25, 880].forEach((freq, i) => {
      const o = ctx!.createOscillator();
      const g = ctx!.createGain();
      o.type = "sine";
      o.frequency.value = freq;
      g.gain.setValueAtTime(0.0001, now + i * 0.22);
      g.gain.exponentialRampToValueAtTime(0.18, now + i * 0.22 + 0.03);
      g.gain.exponentialRampToValueAtTime(0.0001, now + i * 0.22 + 0.9);
      o.connect(g).connect(ctx!.destination);
      o.start(now + i * 0.22);
      o.stop(now + i * 0.22 + 1);
    });
  } catch {
    /* audio may be blocked until the user interacts with the page */
  }
}
