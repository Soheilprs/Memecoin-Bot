#!/bin/sh
# Isolated RPC qualification. Paper only. Do not start EXP004 from this script.
set -eu
cd "$(dirname "$0")/.."
. ./.env
export DATABASE_URL ROBINHOOD_HTTP_URL ROBINHOOD_WS_URL RH_RPC_PRIMARY_HTTP RH_RPC_PRIMARY_WS RH_RPC_FALLBACK_HTTP RH_RPC_FALLBACK_WS RUST_LOG
: "${RUST_LOG:=info}"
exec ./target/release/memecoin-engine research pons-exp001 start --experiment PONS_PROSPECTIVE_EXP004_RPCQUAL
