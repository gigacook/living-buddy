# Tendly

**Fewer things to remember.** Tendly is a calm, self-hostable app for everyday life admin: tasks and household routines, focus timers, shared calendars and lightweight coordination with the people you live or work with. It is designed to be gentle for people who benefit from clear structure, including people with ADHD: one obvious next action, no guilt-based streaks, no productivity rankings.

> "Tendly" is a provisional product name; no trademark search has been done. The repository is called `living-buddy`.

<p align="center"><img src="apps/web/public/favicon.svg" width="96" alt="Pim, Tendly's mascot: a round lavender penguin-like creature with a single antenna"></p>

## What it does

| Area | What works today |
| --- | --- |
| **Today** | One "next up" item, today's list, gentle "from earlier" section, today's events, countdowns, active timer |
| **Tasks & routines** | Categories (Home, Errands, People, School, Work, Personal), priority, duration, due/start/deadline, checklists, tags, filters, history, reassignment with notifications, optimistic-concurrency conflict handling |
| **Household mode** | 12 editable routine templates (gym, groceries, recycling, cleaning, dishes, organizing, laundry, vacuuming, mopping, surfaces, bathroom, bed linen); recurring chores that keep history, skip missed occurrences instead of piling them up, and rotate responsibility |
| **Project mode** | Goals, start/end dates, milestones, customizable Kanban columns (drag-and-drop plus a keyboard-friendly "Move to" menu), timeline view |
| **Groups** | Solo, partners, family, friends, roommates, project teams, custom; opt-in, rate-limited friendly reminders with quiet hours |
| **Focus** | Focus timer, Pomodoro cycles (pause/resume/skip break/+5 min), countdowns, alarms. Timers use absolute deadlines on the server, so they survive reloads and sleep |
| **Calendar** | Month/week/agenda views; `.ics` import; subscriptions to calendar URLs (SSRF-protected); local calendars; filters by person, group, category and source; merged view with de-duplication; ICS and JSON export; revocable read-only share links (HTML, ICS, JSON); change history with attribution |
| **Inbox** | Paste a message or connect a mailbox; suggestions (tasks, appointments, deadlines, follow-ups) are reviewed and confirmed by a person before anything is created |
| **Claude usage card** | Optional manual tracker for 5-hour and 7-day usage windows with checkpoints, reset times and reminders. Manual entry only; no account access |
| **Native app** | Tauri 2 shell that runs the server in-process (no network port, no Node, no Docker) |

See [`build-index.json`](build-index.json) for a per-feature status list with source paths, tests and verification notes.

## Quick start (local)

Requirements: Rust 1.80+ and Node 20+.

```bash
npm install
npm run build                      # builds the web app into apps/web/dist
cargo run -p tendly-server -- seed-demo   # optional: synthetic demo data
TENDLY_WEB_DIR=apps/web/dist cargo run -p tendly-server -- serve
# open http://127.0.0.1:7878
```

For development with hot reload, run `cargo run -p tendly-server -- serve` and `npm run dev` (Vite proxies `/api` to the server) and open http://127.0.0.1:5173. Details: [docs/local-development.md](docs/local-development.md).

## Documentation

- [Local development](docs/local-development.md)
- [Self-hosting](docs/self-host.md): binary, Docker Compose, systemd, cron
- [Security model](docs/security.md): local vs. LAN vs. remote, admin boundary, share links
- [Calendars](docs/calendars.md): import, export, subscriptions, merging, sharing, history
- [Notifications and platform limits](docs/notifications.md)
- [AI extraction and connectors](docs/ai-and-connectors.md): bring your own key, Gmail, Outlook, Slack, Proton
- [Privacy and data flows](docs/privacy.md)
- [Backup and export](docs/backup.md)
- [Platform verification status](docs/platform-status.md)
- [Architecture](docs/architecture.md)

## Repository layout

```
crates/core          Shared business rules (recurrence, ICS, merge, timers, extraction validation, redaction)
crates/server        Axum + SQLite server, worker, CLI (`tendly`)
apps/web             React + Vite + TypeScript frontend
apps/native          Tauri 2 shell (kept out of the Cargo workspace; needs platform SDKs)
packages/contracts   TypeScript API types generated from Rust with ts-rs
integrations         Synthetic fixtures for connector tests
deploy               Docker Compose, systemd and cron examples
```

## Tests

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                 # core + server unit and integration tests
npm run lint && npm run typecheck && npm test
npx playwright test --config apps/web/playwright.config.ts   # e2e + axe accessibility
```

## License

No license has been chosen yet. Until one is added, all rights are reserved by the repository owner.
