#!/usr/bin/env bash
set -euo pipefail

LOCALHOST="127.0.0.1"
RPC_USER="reorg-playground"
RPC_PASSWORD="reorg-playground"
SIGNET_CHALLENGE="5121031b14827738eaf41b67f50a2ddd9d0b08907236f3d0e79bef7fc9ea7b866a3ca821036739e2fc3681d2ef7f9afc0bb2f8964c4f61b6c30f4642cc0322e57967d31f3652ae"

MINER_A_RPC_PORT=38332
MINER_B_RPC_PORT=39332
OBSERVER_RPC_PORT=40332

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

bitcoin_cli() {
  local rpc_port="$1"
  shift

  bitcoin-cli \
    -signet \
    -signetchallenge="$SIGNET_CHALLENGE" \
    -rpcconnect="$LOCALHOST" \
    -rpcport="$rpc_port" \
    -rpcuser="$RPC_USER" \
    -rpcpassword="$RPC_PASSWORD" \
    "$@"
}

stop_node() {
  local node_name="$1"
  local rpc_port="$2"
  local output

  if output="$(bitcoin_cli "$rpc_port" stop 2>&1)"; then
    echo "$node_name: $output"
  else
    echo "$node_name: already stopped or RPC unavailable"
  fi
}

main() {
  require_command bitcoin-cli
  stop_node "Miner A" "$MINER_A_RPC_PORT"
  stop_node "Miner B" "$MINER_B_RPC_PORT"
  stop_node "Observer C" "$OBSERVER_RPC_PORT"
}

main "$@"
