#!/usr/bin/env bash
set -euo pipefail

RPC_USER="${SIGNET_RPC_USER:-reorg-playground}"
RPC_PASSWORD="${SIGNET_RPC_PASSWORD:-reorg-playground}"
RPC_TIMEOUT=10
MINER_WALLET="miner"

SIGNET_CHALLENGE="5121031b14827738eaf41b67f50a2ddd9d0b08907236f3d0e79bef7fc9ea7b866a3ca821036739e2fc3681d2ef7f9afc0bb2f8964c4f61b6c30f4642cc0322e57967d31f3652ae"
MINER_A_DESCRIPTOR="multi(1,cUziKDUPDUDf8XUb9MURQ15Nj71poddNYY4XvUnL4wDqXD2Tj26A,036739e2fc3681d2ef7f9afc0bb2f8964c4f61b6c30f4642cc0322e57967d31f36)"
MINER_B_DESCRIPTOR="multi(1,031b14827738eaf41b67f50a2ddd9d0b08907236f3d0e79bef7fc9ea7b866a3ca8,cV5hR5xMAnzQXrSvs6X6CKt45LrRrtkoJyEfynumw6hK73ivZETk)"

MINER_A_RPC_HOST="${SIGNET_NODE_A_RPC_HOST:-bitcoind-signet-a}"
MINER_A_RPC_PORT="${SIGNET_NODE_A_RPC_PORT:-38332}"
MINER_B_RPC_HOST="${SIGNET_NODE_B_RPC_HOST:-bitcoind-signet-b}"
MINER_B_RPC_PORT="${SIGNET_NODE_B_RPC_PORT:-38332}"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

bitcoin_cli() {
  local rpc_host="$1"
  local rpc_port="$2"
  shift 2

  bitcoin-cli \
    -signet \
    -signetchallenge="$SIGNET_CHALLENGE" \
    -rpcclienttimeout="$RPC_TIMEOUT" \
    -rpcconnect="$rpc_host" \
    -rpcport="$rpc_port" \
    -rpcuser="$RPC_USER" \
    -rpcpassword="$RPC_PASSWORD" \
    "$@"
}

wallet_cli() {
  local rpc_host="$1"
  local rpc_port="$2"
  shift 2

  bitcoin_cli "$rpc_host" "$rpc_port" -rpcwallet="$MINER_WALLET" "$@"
}

wait_for_rpc() {
  local node_name="$1"
  local rpc_host="$2"
  local rpc_port="$3"

  for ((attempt = 0; attempt < 60; attempt++)); do
    if bitcoin_cli "$rpc_host" "$rpc_port" getblockchaininfo >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  echo "$node_name RPC did not become ready in time" >&2
  exit 1
}

ensure_miner_wallet() {
  local rpc_host="$1"
  local rpc_port="$2"

  if bitcoin_cli "$rpc_host" "$rpc_port" listwallets | jq -e --arg wallet "$MINER_WALLET" 'index($wallet) != null' >/dev/null; then
    return 0
  fi

  if bitcoin_cli "$rpc_host" "$rpc_port" loadwallet "$MINER_WALLET" >/dev/null 2>&1; then
    return 0
  fi

  bitcoin_cli "$rpc_host" "$rpc_port" createwallet "$MINER_WALLET" >/dev/null
}

descriptor_with_private_checksum() {
  local rpc_host="$1"
  local rpc_port="$2"
  local descriptor="$3"
  local checksum

  checksum="$(bitcoin_cli "$rpc_host" "$rpc_port" getdescriptorinfo "$descriptor" | jq -er '.checksum')"
  printf '%s#%s\n' "$descriptor" "$checksum"
}

descriptor_with_public_checksum() {
  local rpc_host="$1"
  local rpc_port="$2"
  local descriptor="$3"
  local descriptor_info

  descriptor_info="$(bitcoin_cli "$rpc_host" "$rpc_port" getdescriptorinfo "$descriptor")"
  printf '%s#%s\n' \
    "$(printf '%s' "$descriptor_info" | jq -er '.descriptor')" \
    "$(printf '%s' "$descriptor_info" | jq -er '.checksum')"
}

wallet_has_descriptor() {
  local rpc_host="$1"
  local rpc_port="$2"
  local descriptor="$3"

  wallet_cli "$rpc_host" "$rpc_port" listdescriptors \
    | jq -e --arg descriptor "$descriptor" '.descriptors[] | select(.desc == $descriptor)' >/dev/null
}

ensure_signer_descriptor() {
  local node_name="$1"
  local rpc_host="$2"
  local rpc_port="$3"
  local descriptor="$4"
  local private_descriptor
  local public_descriptor
  local import_result

  private_descriptor="$(descriptor_with_private_checksum "$rpc_host" "$rpc_port" "$descriptor")"
  public_descriptor="$(descriptor_with_public_checksum "$rpc_host" "$rpc_port" "$descriptor")"

  if wallet_has_descriptor "$rpc_host" "$rpc_port" "$public_descriptor"; then
    return 0
  fi

  import_result="$(wallet_cli "$rpc_host" "$rpc_port" importdescriptors "[{\"desc\":\"$private_descriptor\",\"timestamp\":\"now\"}]")"

  if ! printf '%s' "$import_result" | jq -e 'length > 0 and .[0].success == true' >/dev/null; then
    printf '%s' "$import_result" | jq -r '.[0].error.message // "descriptor import failed"' >&2
    echo "Failed to import Signet signer descriptor into $node_name wallet" >&2
    exit 1
  fi
}

main() {
  require_command bitcoin-cli
  require_command jq

  wait_for_rpc "Miner A" "$MINER_A_RPC_HOST" "$MINER_A_RPC_PORT"
  wait_for_rpc "Miner B" "$MINER_B_RPC_HOST" "$MINER_B_RPC_PORT"

  echo "Preparing Signet miner wallets..."
  ensure_miner_wallet "$MINER_A_RPC_HOST" "$MINER_A_RPC_PORT"
  ensure_miner_wallet "$MINER_B_RPC_HOST" "$MINER_B_RPC_PORT"
  ensure_signer_descriptor "Miner A" "$MINER_A_RPC_HOST" "$MINER_A_RPC_PORT" "$MINER_A_DESCRIPTOR"
  ensure_signer_descriptor "Miner B" "$MINER_B_RPC_HOST" "$MINER_B_RPC_PORT" "$MINER_B_DESCRIPTOR"

  echo "Signet miner wallets are ready."
}

main "$@"
