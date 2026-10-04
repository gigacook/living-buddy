# Self-hosting

Tendly is a single binary (`tendly`) plus a static web build. It stores everything in one SQLite database.

Self-hosting is free. If you enable optional integrations, the external providers you connect (for example an AI API you bring your own key for) may charge you.

## Choose one way to run background work

Calendar subscriptions, mailbox connectors and retention clean-up run as background jobs. Pick **one**:

1. **Embedded (default):** `tendly serve` runs the scheduler itself (`TENDLY_EMBEDDED_WORKER=true`).
2. **Dedicated worker service:** `tendly worker` (Docker Compose profile `worker`).
3. **Timer / cron:** `tendly worker --once` from the systemd timer or cron.

For options 2 and 3, set `TENDLY_EMBEDDED_WORKER=false` on the server. Jobs use database leases and idempotency keys, so an accidental overlap won't double-process work, but there is no reason to run two schedulers.

## Binary + systemd

```bash
cargo build --release -p tendly-server
npm ci && npm run build
sudo install -m 0755 target/release/tendly /usr/local/bin/tendly
sudo mkdir -p /usr/local/share/tendly && sudo cp -r apps/web/dist /usr/local/share/tendly/web
sudo useradd --system --home /var/lib/tendly tendly
sudo mkdir -p /etc/tendly && tendly gen-key | sudo tee /etc/tendly/encryption.key >/dev/null
sudo chown root:tendly /etc/tendly/encryption.key && sudo chmod 0640 /etc/tendly/encryption.key
sudo cp deploy/systemd/tendly.service /etc/systemd/system/
sudo systemctl enable --now tendly
```

Optional timer instead of the embedded worker: copy `deploy/systemd/tendly-worker.{service,timer}`, set `TENDLY_EMBEDDED_WORKER=false` in `/etc/tendly/tendly.env`, then `systemctl enable --now tendly-worker.timer`.

Cron alternative: `deploy/cron/tendly.cron`.

## Docker Compose

```bash
cd deploy
cp .env.example .env
mkdir -p secrets && docker run --rm tendly:local gen-key > secrets/encryption.key
docker compose build && docker compose up -d
```

The Compose file uses host networking so Tendly keeps listening on `127.0.0.1:7878` in local mode, with administration available only from that computer. If host networking is not available (some Docker Desktop setups), either run the binary directly or use LAN/remote mode deliberately as described in [security.md](security.md).

## Configuration reference

| Variable | Default | Meaning |
| --- | --- | --- |
| `TENDLY_MODE` | `local` | `local` (loopback only), `lan` (allowlisted private networks), `remote` (paired devices + admin token) |
| `TENDLY_BIND` | `127.0.0.1:7878` | Listen address. Local mode refuses non-loopback addresses |
| `TENDLY_ALLOWED_NETWORKS` | – | LAN mode: comma-separated private CIDRs, e.g. `192.168.1.0/24` |
| `TENDLY_ALLOWED_HOSTS` | – | Extra accepted `Host` names (required in remote mode) |
| `TENDLY_DATA_DIR` | `./data` | Database and generated key location |
| `TENDLY_WEB_DIR` | – | Directory with the built web app to serve |
| `TENDLY_TIMEZONE` | `UTC` | Default IANA time zone |
| `TENDLY_ENCRYPTION_KEY` / `_FILE` / `TENDLY_ENCRYPTION_KEY_PATH` | generated file | 32-byte base64 key for encrypting provider tokens and subscription URLs |
| `TENDLY_ADMIN_TOKEN` / `_FILE` | – | Remote mode admin token (24+ characters) |
| `TENDLY_EMBEDDED_WORKER` | `true` | Run the scheduler inside `serve` |
| `TENDLY_TRUSTED_FETCH_NETWORKS` | – | CIDRs calendar subscriptions may reach despite being private (e.g. a NAS) |
| `TENDLY_FETCH_VIA_PROXY` | `false` | Fetch calendar URLs through the system HTTP proxy (disables IP pinning) |
| `TENDLY_AI_API_KEY` / `_FILE` | – | Bring-your-own AI key (alternative to storing it via Settings) |
| `TENDLY_GOOGLE_CLIENT_ID`, `TENDLY_GOOGLE_CLIENT_SECRET[_FILE]` | – | Your Google OAuth client for the Gmail connector |
| `TENDLY_MICROSOFT_CLIENT_ID`, `TENDLY_MICROSOFT_CLIENT_SECRET[_FILE]`, `TENDLY_MICROSOFT_TENANT` | – | Your Entra ID app for the Outlook connector |
| `TENDLY_OAUTH_REDIRECT_BASE` | `http://127.0.0.1:<port>` | Base URL registered as the OAuth redirect |
| `TENDLY_FIXTURE_DIR` | `integrations/fixtures/mail` | Synthetic mailbox fixtures |
| `TENDLY_DEMO` | `false` | Seed synthetic demo data into an empty database at startup |
| `TENDLY_LOG` | `info` | Log filter (`warn`, `debug`, …). Logs never include message contents or tokens |

`tendly check-config` validates the environment without starting the server. Health endpoints: `/healthz` (process up) and `/readyz` (database and migrations ready).
