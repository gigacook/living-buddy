# Architecture

```
          ┌──────────────── apps/web (React + Vite + TS) ────────────────┐
          │  pages · components · lib/api.ts (typed by packages/contracts)│
          └───────────────┬───────────────────────────────┬──────────────┘
             HTTP (browser)│                               │Tauri command `api_request`
                           ▼                               ▼ (in-process, no socket)
          ┌──────────── crates/server (Axum + SQLx/SQLite) ──────────────┐
          │ security guard: network mode · Host · CSRF · device · admin   │
          │ routes: members groups tasks templates focus calendar shares  │
          │         inbox admin · share pages (HTML/ICS/JSON)             │
          │ calendar store · SSRF-safe fetch · connectors · AI providers  │
          │ worker (durable jobs) · crypto (AES-GCM) · backup/export      │
          └───────────────────────────────┬──────────────────────────────┘
                                          ▼
          ┌──────────── crates/core (pure, no I/O) ───────────────────────┐
          │ recurrence (RRULE, DST) · ics (parse/write) · merge rules      │
          │ timer state machine · alarms · nudges · share scopes           │
          │ extraction validation · redaction · templates · API DTOs       │
          └──────────────────────────────────────────────────────────────┘
```

## Principles

- **One implementation of the rules.** Recurrence, ICS, merging, timers and validation live in `crates/core` and are used by the HTTP server, the worker and the native shell. The frontend only formats what the server returns.
- **Typed contracts.** API types are Rust structs exported to TypeScript with `ts-rs` (`npm run contracts`).
- **Adapters at the edges.**
  - *Transport:* `apps/web/src/lib/transport.ts` uses `fetch` in browsers and the `api_request` Tauri command in the native app.
  - *Notifications:* `apps/web/src/lib/notify.ts` uses the browser Notification API or `tauri-plugin-notification`.
  - *Calendar sources:* `local`, `file`, `url` kinds in `crates/server/src/calendar.rs`; a CalDAV/provider write-back adapter would be a new kind.
  - *AI providers:* `crates/server/src/ai.rs` (`none`, `anthropic`, `openai_compatible`).
  - *Connectors:* `crates/server/src/connectors.rs` (`fixture`, `gmail`, `microsoft_graph`, `slack`, `proton_bridge` scaffold).
  - *Extensions:* `tendly_server::router_with_extensions` merges extra routes *inside* the security guard, so separately maintained modules inherit the same protections.
- **Storage.** SQLite with versioned migrations (`crates/server/migrations`), WAL mode, foreign keys, transactions for multi-row changes, optimistic concurrency (`version`/`revision`) for tasks, timers and events. The path to hosted PostgreSQL: queries are plain SQL through SQLx; the SQLite-specific parts (`INSERT OR IGNORE`, `VACUUM INTO`, the migration DDL) are few and would need porting.
- **Time.** All instants are stored in UTC; dates and times of tasks are stored with their IANA zone; recurrence is expanded in wall-clock time.
