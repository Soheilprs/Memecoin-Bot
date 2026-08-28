#!/bin/sh
# Persistent EXP002 runner. Paper only. No keys. No broadcast.
# Prefer: docker compose --profile exp002 up -d
# Fallback: this script under systemd/tmux on a machine that stays awake.
set -eu
cd "$(dirname "$0")/.."
. ./.env
export DATABASE_URL ROBINHOOD_HTTP_URL ROBINHOOD_WS_URL BASE_HTTP_URL BASE_WS_URL RUST_LOG
: "${RUST_LOG:=info}"
exec ./target/release/memecoin-engine research pons-exp001 start --experiment PONS_PROSPECTIVE_EXP002
