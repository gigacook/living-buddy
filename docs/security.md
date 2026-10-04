# Security model

Tendly separates **who you say you are** (a display name used for attribution) from **who is allowed in** (network boundary or device pairing) and **who may administer** (host-only or admin token).

## Modes

| | Local (default) | LAN | Remote |
| --- | --- | --- | --- |
| Who can connect | This computer only (loopback) | Loopback plus allowlisted **private** networks | Anyone who reaches the URL, but only **paired devices** get past `/api/session` |
| Identity | Pick a display name | Pick a display name | Paired device + display name |
| Administration | Requests from this computer without proxy headers | Same: host-only, never from LAN devices | Paired device **and** `TENDLY_ADMIN_TOKEN` |
| Intended for | One computer, the native app | A home network you trust | Internet access behind HTTPS |

Names are attribution, not authentication. In local and LAN mode anyone who can open Tendly can choose any name; Tendly records who did what by name, which is helpful for households but is not a security control. Stable internal IDs mean renamed or duplicate names don't break assignments.

The server refuses to start in local mode on a non-loopback address, in LAN mode without an allowlist or with a public network in it, and in remote mode without an admin token and an allowed host name.

## Request protections

- **Host validation** blocks DNS-rebinding attacks: only `localhost`, `127.0.0.1`, `[::1]`, the bind address and `TENDLY_ALLOWED_HOSTS` are accepted.
- **CSRF:** state-changing API calls require the `x-tendly-csrf` header (which forces a CORS preflight that is never approved) and, when present, a same-site `Origin`. No CORS headers are sent.
- **Security headers:** strict CSP (no inline scripts), `frame-ancestors 'none'`, `nosniff`, `no-referrer`.
- **Admin boundary:** `/api/admin/*` (AI keys, mailbox connectors, sharing switch, devices, full export) is host-only in local/LAN mode. A loopback connection carrying `X-Forwarded-For`/`Forwarded` headers is treated as not local, so a reverse proxy cannot accidentally grant admin. Do not put LAN mode behind a reverse proxy; use remote mode.
- **Remote mode devices:** pairing codes are created on the host (`tendly device add --name …`), stored as SHA-256 hashes, exchanged for an `HttpOnly; SameSite=Strict; Secure` cookie, and revocable.
- **Rate limits** on pairing, share links, paste intake, member creation and manual refreshes.
- **Errors** are structured and sanitized; internal details are logged with a reference ID and redacted.

## Secrets

- Provider tokens, AI keys and calendar subscription URLs (which often contain secret tokens) are encrypted with AES-256-GCM. The key never lives in the database: it comes from `TENDLY_ENCRYPTION_KEY[_FILE]`, `TENDLY_ENCRYPTION_KEY_PATH`, or an auto-generated owner-only file. Keep it outside backups you share.
- Keys are write-only in the UI and never returned by the API or included in exports.
- Logs pass through a redaction layer (bearer tokens, `token=`/`code=`/`password=` values, provider key formats, share-link tokens, email addresses).
- Nothing secret is stored in browser `localStorage` (it holds only UI preferences and the chosen display name's ID). In remote mode the admin token is kept in memory for the current tab only.

## Share links

Read-only share links are bearer capabilities:

- Off by default; an administrator must enable sharing. Turning it off disables every link immediately.
- 256-bit random tokens, shown once; only their hash is stored.
- Narrow, explicit scopes: specific groups and/or calendars, optional dated tasks, optional *your own* personal tasks, a detail level (busy-only, titles, full) and a time window. Private calendars appear only when picked explicitly.
- Optional expiry; revocation is immediate. Unknown, revoked and expired links return the same 404.
- Token paths are redacted from logs, pages send `no-referrer` and `no-store`. `noindex` is set as a courtesy to crawlers but is **not** access control.

## Untrusted content

Imported calendars, subscribed feeds, emails and chat messages are untrusted data:

- Size, depth and count limits on ICS parsing; all text is escaped when rendered.
- Calendar URL fetching blocks private, loopback, link-local (including cloud metadata), CGNAT, multicast and other special ranges, pins the connection to the validated IPs, re-validates every redirect, and caps size and time. Private networks can be allowed explicitly with `TENDLY_TRUSTED_FETCH_NETWORKS`.
- Message content can never trigger actions. See [ai-and-connectors.md](ai-and-connectors.md).

## Reporting

Please report security issues privately to the repository owner rather than in a public issue.
