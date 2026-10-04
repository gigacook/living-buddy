# Notifications, timers and platform limits

## What Tendly guarantees

- **Timers are correct after reloads, sleep and suspension.** The server stores absolute deadlines. When the app comes back it recomputes the state from the clock. A Pomodoro phase that ended while you were away waits for you ("That part ended while you were away") instead of silently chaining through several phases.
- **Each phase end and each alarm is announced once,** with an on-screen message as well as an optional sound and system notification. Sound and notifications are never the only signal.
- **Missed alarms** (because the app was closed) are shown once when you open Tendly again, labeled as missed.

## What depends on your platform

| Where Tendly runs | Reminders while open | When in the background | When fully closed |
| --- | --- | --- | --- |
| Browser tab | In-page message, chime, browser notification (if allowed) | Browsers may delay timers in background tabs; notifications still appear once the tab runs | **Nothing.** A closed browser cannot ring |
| Desktop app (Tauri) | Same, using OS notifications | Usually works while the app is running | **Nothing** unless the app is running |
| Mobile app (Tauri, iOS/Android) | Same | iOS and Android restrict background execution; reminders may be delayed or skipped | **Not supported** in this version |

Do not rely on Tendly as your only wake-up alarm or medication reminder. Scheduling OS-level notifications ahead of time on mobile (which can fire while the app is closed) is not implemented or verified yet.

## Screen readers and motion

- The timer exposes a `timer` role and a polite live region that announces starts, pauses, phase changes and every five minutes, never every second.
- Reduced motion is honored from the OS setting and from Settings → "Reduce motion". There is no flashing content.

## Cooperative reminders

People in a group can send each other a gentle reminder about a task if the recipient opts in (Settings → "Let people in my groups send me gentle reminders"). Limits: one per task per six hours and five per sender per day. Reminders that arrive during the recipient's quiet hours wait until quiet hours end. There are no rankings or productivity statistics.
