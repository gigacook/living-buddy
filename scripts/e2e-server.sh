#!/usr/bin/env bash
# Starts a throwaway Tendly server for end-to-end tests: fresh data directory,
# production web build, loopback only, sharing off by default.
set -euo pipefail
PORT="${1:-7899}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="$(mktemp -d)"
trap 'rm -rf "$DATA"' EXIT
cd "$ROOT"
[ -f apps/web/dist/index.html ] || npm run build -w @tendly/web
cargo build -q -p tendly-server
export TENDLY_BIND="127.0.0.1:${PORT}"
export TENDLY_DATA_DIR="$DATA"
export TENDLY_WEB_DIR="$ROOT/apps/web/dist"
export TENDLY_FIXTURE_DIR="$ROOT/integrations/fixtures/mail"
export TENDLY_EMBEDDED_WORKER=false
export TENDLY_LOG="${TENDLY_LOG:-warn}"
exec "$ROOT/target/debug/tendly" serve
