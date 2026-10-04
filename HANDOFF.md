# Tendly — continuation handoff

## 1. Mission and authority

You are continuing an existing product build. Your job is to implement, integrate, verify, and close out the agreed product scope—not merely follow an inherited TODO list.

Treat the original product requirements as the target, the workspace as evidence, and the previous session's plan as provisional.

Start by inspecting the available artifacts and validating the handoff. Then execute. Do not stop after producing a plan, completing the first listed task, or presenting another general progress summary.

Owner preferences stated explicitly during the first session (they override defaults):

- Work autonomously; do not ask clarification questions for routine choices.
- Commit finished, verified work and push it to **`main`** of `github.com/gigacook/living-buddy` (fast-forward only; never force-push, never rewrite history). The owner was annoyed that the first session worked on a side branch.
- Running, installing and testing must be **one clean command**, not several lines of npm/cargo. The first session added `./tendly` for this; keep it the front door and extend it rather than adding multi-step instructions.
- The owner is terse; keep status messages short and concrete.
- All product UI, code comments, docs, fixtures and commit messages in English.

## 2. Product completion contract

### What the product is

**Tendly** (provisional name, no trademark search; repository still called `living-buddy`; renaming the GitHub repo is an owner action) is a calm, visually simple life-administration and focus app for people who benefit from clear structure, including people with ADHD. Promise: "fewer things to remember", not another demanding productivity system. Mascot: **Pim**, an original pastel penguin-like creature with a rounded body, side flippers and one antenna (original SVG, several poses), used sparingly (onboarding, tips, empty states), dismissible, with a quiet mode and reduced motion.

Users: individuals, partners, families, friends, roommates, small project teams, custom groups, on a trusted local computer or LAN; later a protected remote/hosted mode.

### Principal journeys (each must work end to end)

1. First run: enter a display name (no account/password) → land on Today with one obvious next action.
2. Capture and manage tasks: title, notes, category (Home, Errands, People, School, Work, Personal — distinct accessible pastel identities on content, neutral pickers, never colour-only), priority, duration, owner/assignee, due/start/deadline, recurrence, checklist, tags, filters, group; history, reassignment, completion history, conflict handling.
3. Household routines: 12 editable templates (gym, groceries, waste/recycling, general cleaning, dishes, organizing, laundry, vacuuming, mopping, wiping surfaces, bathroom, bed linen); completing an occurrence keeps history and schedules the next; overdue/upcoming without shaming; simple rotation.
4. Projects: goal, milestones, timeframe, deadlines; Kanban (backlog/planned/in progress/blocked/done, customizable); timeline; assignment.
5. Groups: create a group of any kind, scope tasks/calendars to it, send opt-in cooperative reminders with quiet hours and anti-spam limits. No surveillance or rankings.
6. Focus: focus timer, Pomodoro (configurable; pause/resume/reset/skip break; presets), countdowns, alarm-style reminders, optional task link, state persisted with absolute deadlines and correct recovery after reload/sleep, notification permission flow, sound controls with visual alternatives, no per-second screen-reader announcements.
7. Calendar: month/week/agenda HTML views; ICS import; URL subscriptions; multiple sources; filters (person, group, category, date range, source); ICS + JSON export; merged views/exports with deterministic dedupe and local overrides; never silently delete unrelated events; revocable, scoped, expiring read-only share links (off by default); change history with actor (or "external source / unknown actor"), timestamp, source, operation, revision, before/after. Correct ICS: stable UIDs, DTSTAMP/LAST-MODIFIED/SEQUENCE, time zones and DST, all-day, recurrence + exclusions, cancellations, provenance, escaping, safe parsing. SSRF-safe fetching. Distinguish publishing a feed, polling a feed, and provider write-back (write-back only where truly supported; CalDAV boundary designed, honestly marked).
8. Inbox: paste text or connect a mailbox/messaging source (self-host, optional) → AI or rule-based extraction → reviewable suggestions (tasks, appointments, deadlines, follow-ups, source references, explicit unknown fields) → person confirms before anything changes. Content is untrusted: cannot override rules, run tools, reveal secrets, send messages, change permissions, or trigger purchases/irreversible actions. No automatic attachment processing; minimal content to providers; clear consent.
9. Optional Claude usage card: manual 5-hour window (25/50/75/100% checkpoints) and 7-day window (50/100%), reset times, notes, last-updated + source, optional reminder thresholds, labelled as estimates. No scraping, no session cookies, no invented quota API.
10. Settings/admin: network mode, sharing kill switch, AI provider (BYOK, keys server-side only), connectors, devices, backup/export, notification and calm-motion preferences.

### Required constraints

- Rust backend and shared business logic (Axum, Tokio, Serde, SQLx + SQLite, versioned migrations, path to PostgreSQL later); Tauri 2 native shell (browser, macOS, Windows, iOS, Android targets); React + Vite + TypeScript frontend. Business rules live once in Rust, not reimplemented in TS.
- A native local install must not need a Node dev server or Docker for tasks and timers.
- Security modes: loopback-only by default; LAN explicit with subnet allowlist, Host validation, CORS/CSRF protection; protected remote mode with real device/workspace authentication. Provider tokens, API keys, mailbox settings, admin and sensitive integration controls are never exposed just because someone knows a display name; admin stays loopback-only or behind a separate authenticated boundary. Share links are bearer capabilities (high entropy, narrow scope, revocable, redacted in logs). No unauthenticated multi-tenant backend.
- Self-host connector worker with durable jobs: Docker Compose, systemd service/timer, cron examples (alternatives, not simultaneous). Gmail (OAuth), Outlook/M365 (Graph, OAuth), Proton (user-run Bridge), Slack (supported APIs and authorization flow), extensible interface. Least-privilege scopes, tokens encrypted with keys outside the DB, refresh/revocation/reconnect, cursors, dedupe, backoff, retry limits, idempotency, enable/disable, health states, retention/deletion, no content or tokens in logs. No cookie/browser-session harvesting.
- WCAG 2.2 AA target: semantic HTML, keyboard, visible focus, focus management, accessible dialogs, labels/errors, contrast despite pastels, ≥44×44 px targets, reduced motion, no flashing.
- Typed API contracts, input validation, sanitized errors, transactions, optimistic concurrency, safe logging, health/readiness endpoints, synthetic demo data, backup/restore/export, lockfiles, reproducible setup. Least-privilege Tauri capabilities (no shell, no broad filesystem).
- Honesty: never claim closed-app/background alarms, real-device verification, or working provider integrations without evidence. Avoid fake buttons and screens.

### Public repository rules

- This repository is public. Planning documents, non-public modules and anything not meant for release stay **outside the Git working tree and outside every public build context** (Docker context, frontend bundle, CI artifacts, screenshots, docs, public manifests). A `private/` folder or `.gitignore` inside the checkout is not sufficient.
- Never commit anywhere: API keys, payment credentials, webhook secrets, OAuth secrets, session cookies, refresh tokens, encryption keys, production connection strings, real mailbox/calendar/user data, sensitive logs. Use environment variables or `*_FILE` secret files; commit only obvious placeholders.
- The project must build and run from this repository alone.
- Before every push: inspect the diff and the history being published, run a secret scanner if available, and manually check for material that should not be public.
- Do not charge anyone, create paid infrastructure, contact real users, connect production inboxes, or change repository visibility.
- Public privacy docs must accurately describe data processing, external AI use and telemetry (there is no telemetry today).

### Completion level and acceptance criteria

Target: a **locally working, self-hostable product with deployment-ready scaffolding**, verified by automated tests, plus native packaging verified where SDKs exist. Not an enterprise platform; a hosted launch and managed (hosted) mailbox connectors are out of scope for this repository.

Acceptance criteria:

1. `./tendly demo` on a fresh clone (Rust 1.80+, Node 20+) builds and serves the full app at http://127.0.0.1:7878; `./tendly test` runs every CI check and passes.
2. Each journey 1–10 above works end to end in the browser against the real server with persistence across restart — exercised by Playwright, not only unit tests.
3. Calendar correctness covered by tests: ICS round trips, recurring/all-day events, DST boundaries, merge dedupe and conflict rules, share revocation, filtering.
4. Security boundaries covered by tests: modes, Host/CSRF, admin boundary, secret redaction, SSRF, untrusted content, connector retries and duplicate ingestion.
5. axe WCAG 2.2 AA scans clean on every page, light and dark, desktop and phone viewport; keyboard-only journeys work.
6. Native desktop app builds and runs on the platforms available; others are honestly labelled "configuration only".
7. Self-host paths (Docker Compose, systemd, cron) are runnable as documented, with at least Compose verified end to end where Docker is available.
8. Public docs and `build-index.json` accurately reflect status.
9. CI on `main` green.

## 3. Workspace and reproducibility

### Access

- Public repo: `https://github.com/gigacook/living-buddy` (public; do not change its visibility). Branch to use: `main`. A leftover branch `claude/living-buddy-build-6w7uwy` points at an older commit and can be ignored or deleted.
- At the time of writing `main` contains: `1a12f65` Initial commit → `8e33278` core build → `7c2ad9c` Tauri shell/frontend/e2e → `8147217` deploy/CI/docs/index → `a83d280` index CI verification → a final commit adding `./tendly`, docs updates and the sanitized `HANDOFF.md` (run `git log --oneline -8` to confirm).
- CI: `.github/workflows/ci.yml`, jobs `rust`, `web` (incl. Playwright), `native-linux`; triggers on push to `main` and `claude/**`, and on PRs. Last verified green: run 37164064826 on `a83d280` (side branch). The run on `main` for `a83d280` was queued when this was written; check it.
- Anything else from the first session (running servers, `/tmp` files, Docker images, scratch Dockerfiles, Xvfb) is gone. Do not assume any of it.

### Stack and versions (as pinned in lockfiles)

- Rust stable (built with cargo 1.97; minimum 1.80), workspace `Cargo.toml` with members `crates/core`, `crates/server`; `apps/native/src-tauri` is excluded and has its own `Cargo.lock`. `rustfmt.toml` max_width 140.
- Axum 0.8, Tokio, SQLx 0.8 (sqlite), reqwest (rustls), clap CLI binary `tendly`, ts-rs for TS contracts (`.cargo/config.toml` sets `TS_RS_EXPORT_DIR` → `packages/contracts/src/generated`).
- Node 20+ (CI uses 22), npm workspaces: `packages/contracts`, `apps/web`, `apps/native`. React 19, react-router 7, TanStack Query 5, lucide-react, Vite 7, Vitest 5, ESLint (jsx-a11y), Playwright 1.56.1 + @axe-core/playwright, Tauri 2 (`@tauri-apps/cli`, `@tauri-apps/api`, `@tauri-apps/plugin-notification`).

### Commands

| Purpose | Command |
| --- | --- |
| Install + build + run (port 7878, data in `./data`) | `./tendly` |
| Same with synthetic demo data on first run | `./tendly demo` |
| Dev (API 7878 + Vite 5173 with `/api` proxy) | `./tendly dev` |
| All CI checks | `./tendly test` (`./tendly test quick` skips Playwright) |
| Native desktop build | `./tendly app` (runs `npx tauri build` in `apps/native`) |
| Rust only | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo test --workspace --locked` |
| Web only | `npm run lint`, `npm run typecheck`, `npm test`, `npm run build` |
| E2E + axe | `cd apps/web && npx playwright test` (starts `scripts/e2e-server.sh` on port 7899 with a throwaway data dir) |
| Regenerate TS contracts | `npm run contracts` (CI fails if `packages/contracts` is stale) |
| Server CLI | `tendly serve | worker [--once] [--interval-secs N] | backup --out F | restore | export --out F | seed-demo | device add/list/... | gen-key | check-config` |
| Docker | `cd deploy && cp .env.example .env && mkdir -p secrets && docker run --rm tendly:local gen-key > secrets/encryption.key && docker compose build && docker compose up -d` (host networking; see `docs/self-host.md`) |

In the Claude Code cloud container: Chromium for Playwright is preinstalled at `/opt/pw-browsers` (do not run `playwright install`); WebKit is **not** available there. Outbound HTTPS goes through a proxy with a custom CA (`/root/.ccr/ca-bundle.crt`); Docker builds inside that container need the CA injected (the first session used a scratch Dockerfile with `--build-context ca=/root/.ccr`, not committed) and `dockerd` had to be started manually.

### Configuration (names only; never commit values)

`TENDLY_MODE` (local|lan|remote), `TENDLY_BIND`, `TENDLY_ALLOWED_NETWORKS`, `TENDLY_ALLOWED_HOSTS`, `TENDLY_DATA_DIR`, `TENDLY_DATABASE_PATH`, `TENDLY_WEB_DIR`, `TENDLY_TIMEZONE`, `TENDLY_ENCRYPTION_KEY` / `_FILE` / `TENDLY_ENCRYPTION_KEY_PATH`, `TENDLY_ADMIN_TOKEN` / `_FILE`, `TENDLY_COOKIE_SECURE`, `TENDLY_EMBEDDED_WORKER`, `TENDLY_TRUSTED_FETCH_NETWORKS`, `TENDLY_FETCH_VIA_PROXY`, `TENDLY_FIXTURE_DIR`, `TENDLY_DEMO`, `TENDLY_LOG`, `TENDLY_AI_API_KEY` / `_FILE`, `TENDLY_GOOGLE_CLIENT_ID`, `TENDLY_GOOGLE_CLIENT_SECRET` / `_FILE`, `TENDLY_MICROSOFT_CLIENT_ID`, `TENDLY_MICROSOFT_CLIENT_SECRET` / `_FILE`, `TENDLY_MICROSOFT_TENANT`, `TENDLY_OAUTH_REDIRECT_BASE`. Reference table: `docs/self-host.md`. Example file: `deploy/.env.example`. Encryption key auto-generated at `<data>/secrets/encryption.key` on first run if none is supplied.

Database: SQLite, WAL, migrations in `crates/server/migrations/0001_initial.sql` (single migration so far; templates seeded by `crates/server/src/db.rs`). No external services needed for the core; provider integrations need the owner's own OAuth apps/keys (none exist).

Deployment: none. No authorized deployment target exists; nothing is deployed.

### Key files and boundaries

- `crates/core/src/` — pure logic, no I/O: `recurrence.rs` (RRULE subset, DST gap/overlap via `resolve_local`), `ics.rs` (parse/write, folding, escaping, VTIMEZONE generation, stable UIDs), `merge.rs` (SEQUENCE > LAST-MODIFIED > content hash; per-source removal; deterministic dedupe winner), `timer.rs` (state machine with absolute deadlines; `phase_complete` after >90 s absence), `alarm.rs`, `notify.rs` (quiet hours, anti-spam), `rotation.rs`, `share.rs`, `templates.rs`, `usage.rs`, `redact.rs`, `extraction.rs` (strict output validation, injection flags), `api.rs`/`model.rs` (DTOs exported via ts-rs).
- `crates/server/src/` — `lib.rs` (`init_state`, `router`, `router_with_extensions`), `main.rs` (CLI), `config.rs`, `security.rs` (mode, Host, CSRF header `x-tendly-csrf` + Origin, device cookie `tendly_device` in remote mode, admin boundary: trusted local = loopback without forwarding headers; remote needs `x-tendly-admin-token` + device; actor header `x-tendly-actor`; CSP), `crypto.rs` (AES-256-GCM), `fetch.rs` (SSRF-safe: IP blocklists, DNS pinning, manual redirects, size limits), `calendar.rs`, `share_render.rs`, `ai.rs` (none / anthropic / openai_compatible), `connectors.rs` (fixture, gmail, microsoft_graph, slack, proton_bridge scaffold), `worker.rs` (durable jobs, leases, backoff+jitter, idempotency keys, dead after 5), `backup.rs`, `seed.rs`, `activity.rs`, `validate.rs`, `routes/*.rs`. Health: `/healthz`, `/readyz`.
- `crates/server/tests/` — `security.rs`, `tasks.rs`, `calendar.rs`, `connectors.rs` (incl. Gmail/Slack/Graph against local mock servers), `common/mod.rs`.
- `apps/web/src/` — `App.tsx`, `pages/{Onboarding,PairDevice,Today,Tasks,Focus,Calendar,Groups,GroupDetail,Inbox,Settings,Notifications}.tsx`, `components/` (Mascot, TaskItem, TaskEditor, UsageCard, Watchers, calendar dialogs), `lib/` (`transport.ts` fetch vs Tauri `api_request`, `api.ts`, `notify.ts`, `useTimer.ts`, `prefs.ts` — localStorage for non-secret prefs only, `contrast.ts`), `styles/tokens.css` + `app.css`. E2E: `apps/web/e2e/app.spec.ts`.
- `apps/native/src-tauri/` — `src/lib.rs` dispatches `api_request` in-process to the same Axum router (tower `oneshot`, loopback ConnectInfo, Host `tauri.localhost`), data in the OS app-data dir, embedded worker every 120 s; capabilities `core:default`, `notification:default`, `allow-api-request` only; icons generated.
- `integrations/fixtures/mail/sample/001–006` — synthetic messages (including a prompt-injection sample and a duplicate).
- `deploy/` — `docker-compose.yml`, `.env.example`, `systemd/{tendly.service,tendly-worker.service,tendly-worker.timer}`, `cron/tendly.cron`. `Dockerfile` at root (node build → rust build → debian-slim, non-root uid 10001).
- Docs: `README.md`, `SECURITY.md`, `docs/{local-development,self-host,security,calendars,notifications,ai-and-connectors,privacy,backup,platform-status,architecture}.md`. Public feature index: `build-index.json` (59 entries; normally regenerated by an owner-held tool).
- `./tendly` — the one-command entry point (bash; macOS/Linux; no Windows equivalent yet).

## 4. Actual build state

Evidence below comes from the first session's runs on Linux x86_64 (the cloud container) and GitHub CI. Re-run before relying on it.

### Verified working (automated tests + manual/browser checks)

- Rust: 182 tests passing (core 138, server unit 17, integration: security 7, tasks 8, calendar 6, connectors 6); fmt and clippy `-D warnings` clean; CI green on `a83d280`.
- Web: ESLint, `tsc`, 36 Vitest unit/component tests (incl. WCAG contrast checks parsing `tokens.css`), production build.
- Playwright: 15 passing, 1 skipped by design, in Chromium at 1280×860 and Pixel 7 emulation; axe WCAG 2.2 AA clean on all pages in light and dark; asserts no horizontal page scroll; skip link, dialog focus. The 8 tests (each run on desktop and phone projects): onboarding + quick add + complete/undo; routine from a template rotates and moves to its next date; project board add card + keyboard "Move to"; focus timer survives reload and pauses; calendar import, agenda, export and revocable share link; inbox paste → confirm suggestion; axe on every main page; keyboard skip link + dialog focus (desktop only — the skipped one). **Not covered by any browser test:** groups and nudges, the usage card, settings/admin (AI provider, connectors, devices, backup), remote-mode pairing, calendar subscriptions/filters/history UI, conflict UI, alarms/countdowns.
- Covered by Rust tests: timer persistence and sleep recovery, recurring chores, reassignment/history, Kanban transitions, group scoping, calendar filtering, share revocation, ICS round trips, recurring/all-day/DST, merge dedupe and conflicts, secret redaction, admin/authorization boundaries, SSRF, connector retries and duplicate ingestion, untrusted content handling, Gmail/Graph/Slack adapters against **mock** servers.
- Native Linux: Tauri app compiled and launched under Xvfb; UI rendered and created its DB via the in-process API. Not packaged, not used interactively.
- Docker image: built and run in the container (healthy, non-root, refuses non-loopback bind in local mode). Compose itself not run end to end.
- `./tendly demo` (added at the end of session 1) built and served the app from the working checkout (UI HTML served, demo members returned by `/api/members`). Fresh-clone run and `./tendly test` results: see the "Last session's final checks" line at the end of this handoff.
- Secret scanning (detect-secrets) on the public tree: only synthetic fixtures flagged. Manual check found no private terms in public files.

### Implemented but unverified with real services

- Gmail connector (OAuth PKCE, history cursor, refresh, revocation) and Outlook/M365 (Graph delta, OAuth PKCE): only mock-server tests; need the owner's OAuth apps. Google may require app verification for restricted scopes.
- BYOK AI: Anthropic adapter (Messages API with structured JSON output and refusal handling) and OpenAI-compatible adapter: never called with a real key.
- Native notifications (`tauri-plugin-notification`): wired, not verified on any OS.

### Partial

- Alarms ring only while the app/tab is open; no OS-scheduled notifications for closed apps on any platform.
- Slack: bot-token channel history works against mocks; no OAuth install flow.
- Tauri native: Linux build only; macOS/Windows/iOS/Android configuration only; `tauri ios init` / `tauri android init` never run; mobile build of the in-process server (sqlx/rustls on iOS/Android) unproven.
- Accessibility: automated axe + keyboard checks only; no screen-reader (NVDA/VoiceOver/TalkBack) testing.
- Project timeline: simple horizontal view; no dependencies or drag-to-reschedule.
- Conflict resolution: person chooses "theirs/mine"; no field-level merge.
- Nudges between members: in-app plus a system notification while the app is open; no push service (so a partner on another device only sees it when their app is open).
- Secrets at rest: AES-GCM with a key file outside the DB; OS keychain not used (native app stores the key file in its app-data dir).

### Scaffolded / missing / blocked

- Proton Mail Bridge connector: configuration scaffold only; IMAP reader not implemented.
- CalDAV adapter and any provider write-back: not started (boundary documented in `docs/calendars.md`).
- Official Claude usage integration: blocked by design (no supported consumer API); manual card is the deliverable.
- systemd/cron examples: written, never run.
- Public deployment: blocked (no authorized target). Nothing is deployed.
- Safari/WebKit: not verified (no WebKit available in the build container).
- No `LICENSE` file — licensing is an open owner decision; do not pick one silently.
- `./tendly` is bash-only; Windows users have no one-command path (could add `tendly.ps1` or rely on the native installer).

### Known traps and decisions worth keeping

- `TimerCommand` serde uses tag `action` and `rename_all_fields = "camelCase"`; snake_case fields were silently ignored before this fix.
- SQLite `COALESCE(MAX(position),0)+1` decodes as integer; cast to REAL for f64 positions.
- Default quiet hours made notification tests time-dependent; tests clear quiet hours; self "usage" notifications bypass quiet hours.
- Native `<dialog>.showModal()` focuses the Close button; the app moves focus to `[data-autofocus]` or the first field.
- Horizontal scroll on phone came from absolutely positioned visually-hidden labels in scroll containers; containers are `position: relative` and grid/stack children `min-width: 0`. E2E asserts no page-level horizontal scroll — keep it.
- Main element must not grab focus on first render or the skip link stops being the first Tab stop.
- Category colours appear on content surfaces only; pickers stay neutral (explicit requirement).
- Business rules stay in `crates/core`; the frontend formats only.
- `ServerMode` is `Copy`; clippy runs with `-D warnings` (crate-level `allow(clippy::type_complexity)` in server).

### Owner-held material outside this repository

The owner keeps some planning material and non-public modules outside this repository. Nothing here depends on them; the public project builds and runs without them. If your session was given them, follow the instructions that come with them, keep them outside this checkout, and never copy them into this repository, a Docker build context, CI artifacts, screenshots or docs.

## 5. Independent product-gap assessment

### A. Known gaps (from the first session)

Priority 1 — integrity/security:
- Confirm remote mode end to end in a browser (device pairing, admin token, CSRF) — integration tests exist but no Playwright journey.
- Confirm no secrets reach the browser bundle or logs after any change (grep the `dist/` output and run the redaction tests).

Priority 2 — required journeys/integrations still short of the product contract:
- Alarms/reminders when the app is closed: at least implement OS-scheduled notifications in the native app where Tauri supports scheduling (check what `tauri-plugin-notification` v2 actually supports per platform before promising), and be explicit in UI copy where it cannot.
- Proton via Bridge: implement the IMAP reader (STARTTLS to the local Bridge, UID-based cursor, dedupe) with a mock IMAP test, or leave scaffolded and documented.
- Slack OAuth install flow (or document bot-token-only as the self-host path and mark it so in UI).
- Group nudges across devices: today only visible when the recipient's app is open. Decide whether LAN polling + in-app delivery is sufficient for the product contract (likely yes for local/LAN) and make the UI copy match.
- Native: run `tauri ios init`/`android init` only where SDKs exist; otherwise keep "configuration only".

Priority 3 — verification/reproducibility:
- Run Docker Compose end to end where Docker exists; run the systemd unit and cron line at least in a container or VM if feasible.
- WebKit: if Playwright WebKit can be obtained in the environment, add a WebKit project to `playwright.config.ts`; otherwise keep it labelled unverified.
- Windows one-command path (`tendly.ps1`) if cheap.
- Package the Linux native app (`.deb`/AppImage via `./tendly app`) and smoke-test it.

Priority 4 — finishing:
- Timeline improvements only if a journey is actually blocked.

### B. Likely omissions to inspect before deciding they need work

- Empty, loading and error states on every page (including offline server / failed fetch), and recovery after a server restart while the UI is open.
- Editing and deleting recurring calendar events (single occurrence vs series), and local overrides surviving feed refresh in the UI, not only in tests.
- Task deletion/undo and archive of done items; long lists performance.
- Time zone per member vs default zone in the UI.
- Multi-user on the same LAN: two browsers editing the same task triggering the conflict UI (write a Playwright test with two contexts).
- Share link pages (HTML) accessibility and phone layout.
- Import of large or hostile ICS files from the UI (size limits surfaced as friendly errors).
- Backup/restore round trip from the CLI and from Settings (if exposed), including restored DB opening with the same key.
- PWA/installability (manifest exists; no service worker — probably fine; do not add offline sync speculatively).
- The usage card's reminder thresholds actually firing notifications.
- Documentation drift after `./tendly` (README, local-development, self-host).

Do not promote speculative features (new integrations, analytics, hosted launch) into scope.

## 6. Execution plan and operating rules

### Starting sequence

1. `git status && git log --oneline -8` on `main`; read `README.md`, `docs/platform-status.md`, `build-index.json`, `apps/web/e2e/app.spec.ts`.
2. If the owner supplied additional material outside this repository, set it up outside the checkout as its own instructions say; otherwise continue without it.
3. Run `./tendly test`. If anything fails, fixing it is the first task.
4. Run `./tendly demo` on a fresh data dir (`TENDLY_DATA_DIR=$(mktemp -d)`) and walk every journey in section 2 with Playwright (or a scripted browser), desktop and phone viewport, light and dark. Write down every broken, fake or confusing step.
5. Build a completion checklist from section 2 acceptance criteria plus what step 4 found; reconcile with section 5.
6. Work through it in priority order as connected slices (server + UI + test + docs + index for each), committing after each verified slice.
7. Re-run the full suite, update `docs/platform-status.md`, `build-index.json` (normally regenerated by an owner-held tool; if it is not available, edit the public index by hand and say so in `HANDOFF.md`), and `HANDOFF.md`.
8. Pre-push review (diff, history, secret scan, manual private-material check), push to `main`, confirm CI green.

### Operating rules

- Inspect first; reconcile this handoff with the actual workspace.
- Derive a completion checklist from the product contract and observed gaps.
- Preserve working behavior and explicit user decisions.
- Prefer targeted repairs over unnecessary rewrites.
- Revise the plan when evidence shows that the inherited plan is inadequate.
- Implement connected, end-to-end slices rather than accumulating isolated scaffolding.
- After meaningful changes, run the relevant checks and fix failures.
- Use available browser/runtime tools to test real journeys where appropriate.
- Recheck the whole completion contract before declaring completion.
- Keep `HANDOFF.md` current at meaningful milestones (public-safe version only in the repo).

### Autonomy

- Do not ask the user to prioritize routine work, repeat known requirements, or perform checks that available tools can perform.
- Resolve low-risk, reversible ambiguity using reasonable defaults consistent with the product; record material assumptions.
- Replace unnecessary "needs user check" items with executable verification.
- Never fabricate credentials, external access, test evidence, or user acceptance.
- Do not treat mocks as proof that real integrations work.
- Ask only when progress genuinely requires unavailable information, a consequential product decision (e.g. license, repo rename), authorization, or an irreversible/high-impact action.
- Do not purchase, publish, deploy to production, delete important data, or change external accounts without authorization.
- When one area is blocked, complete independent work rather than stopping the entire build.

## 7. Verification and stopping conditions

Completion checks for this product:

- `./tendly test` green (fmt, clippy `-D warnings`, all Rust tests, ESLint, typecheck, Vitest, production build, Playwright + axe).
- Fresh-clone `./tendly demo` serves a working app; every journey in section 2 exercised in a browser against the real server with a restart in between to prove persistence.
- New behavior has tests at the right level (core unit, server integration, Playwright journey).
- Security regressions checked: modes, Host/CSRF, admin boundary, share revocation, SSRF, redaction, untrusted content.
- Docs and both indexes match reality; `docs/platform-status.md` distinguishes verified, emulated, built-only and configuration-only.
- Public push reviewed (diff, history, secret scan, manual private check); CI green on `main`.

Loop implement → run → inspect → fix until the criteria are met or a genuine external blocker (missing SDK, credentials, authorization) stops a specific item; then continue with everything else. "Initial tasks completed" is not a stopping condition. Passing unit tests or a successful build alone do not prove a journey works.

Final closeout must state: what is complete; what was run and the results; remaining limitations, failures and unverified behavior; exact run/use instructions (`./tendly …`); only unavoidable owner actions (each with reason and minimum input — e.g. OAuth app credentials, license choice, repo rename, a deployment target). Distinguish implementation-complete, locally verified, release-ready and user-accepted; claim nothing stronger than the evidence.

If you must stop early, update `HANDOFF.md` with: current state, last completed change and check, any broken in-progress state, next concrete action, remaining acceptance criteria, exact blockers.

---

Last session's final checks (2026-10-04, Linux x86_64 cloud container): `./tendly test` passed end to end (Rust 182 tests across 9 test binaries, fmt + clippy clean; ESLint, tsc, 36 Vitest tests, production build; Playwright 15 passed / 1 skipped incl. axe). `./tendly demo` on a fresh `git clone` (no `node_modules`, no build output) installed dependencies, built the web app and the release server (about 8 minutes cold), seeded demo data and served the app: `/healthz` returned `ok`, `/api/members` returned the demo people, `/` served the UI.
