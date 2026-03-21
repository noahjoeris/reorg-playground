#!/usr/bin/env bash
set -euo pipefail

LOCALHOST="127.0.0.1"
RPC_USER="reorg-playground"
RPC_PASSWORD="reorg-playground"
RPC_TIMEOUT=5
MINER_WALLET="miner"
SIGNET_ENABLE_DEFAULT_PEERS="${SIGNET_ENABLE_DEFAULT_PEERS:-1}"

SIGNET_CHALLENGE="5121031b14827738eaf41b67f50a2ddd9d0b08907236f3d0e79bef7fc9ea7b866a3ca821036739e2fc3681d2ef7f9afc0bb2f8964c4f61b6c30f4642cc0322e57967d31f3652ae"
MINER_A_DESCRIPTOR="multi(1,cUziKDUPDUDf8XUb9MURQ15Nj71poddNYY4XvUnL4wDqXD2Tj26A,036739e2fc3681d2ef7f9afc0bb2f8964c4f61b6c30f4642cc0322e57967d31f36)"
MINER_B_DESCRIPTOR="multi(1,031b14827738eaf41b67f50a2ddd9d0b08907236f3d0e79bef7fc9ea7b866a3ca8,cV5hR5xMAnzQXrSvs6X6CKt45LrRrtkoJyEfynumw6hK73ivZETk)"

MINER_A_RPC_PORT=38332
MINER_A_P2P_PORT=38333
MINER_A_DATA_DIR="$HOME/Library/Application Support/Bitcoin/signet-cluster-minerA"

MINER_B_RPC_PORT=39332
MINER_B_P2P_PORT=39333
MINER_B_DATA_DIR="$HOME/Library/Application Support/Bitcoin/signet-cluster-minerB"

OBSERVER_RPC_PORT=40332
OBSERVER_P2P_PORT=40333
OBSERVER_DATA_DIR="$HOME/Library/Application Support/Bitcoin/signet-cluster-observerC"

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
    -rpcclienttimeout="$RPC_TIMEOUT" \
    -rpcconnect="$LOCALHOST" \
    -rpcport="$rpc_port" \
    -rpcuser="$RPC_USER" \
    -rpcpassword="$RPC_PASSWORD" \
    "$@"
}

wallet_cli() {
  local rpc_port="$1"
  shift

  bitcoin_cli "$rpc_port" -rpcwallet="$MINER_WALLET" "$@"
}

ensure_miner_wallet() {
  local rpc_port="$1"

  if bitcoin_cli "$rpc_port" listwallets | jq -e --arg wallet "$MINER_WALLET" 'index($wallet) != null' >/dev/null; then
    return 0
  fi

  if bitcoin_cli "$rpc_port" loadwallet "$MINER_WALLET" >/dev/null 2>&1; then
    return 0
  fi

  bitcoin_cli "$rpc_port" createwallet "$MINER_WALLET" >/dev/null
}

descriptor_with_private_checksum() {
  local rpc_port="$1"
  local descriptor="$2"
  local checksum

  checksum="$(bitcoin_cli "$rpc_port" getdescriptorinfo "$descriptor" | jq -er '.checksum')"
  printf '%s#%s\n' "$descriptor" "$checksum"
}

descriptor_with_public_checksum() {
  local rpc_port="$1"
  local descriptor="$2"
  local descriptor_info

  descriptor_info="$(bitcoin_cli "$rpc_port" getdescriptorinfo "$descriptor")"
  printf '%s#%s\n' \
    "$(printf '%s' "$descriptor_info" | jq -er '.descriptor')" \
    "$(printf '%s' "$descriptor_info" | jq -er '.checksum')"
}

wallet_has_descriptor() {
  local rpc_port="$1"
  local descriptor="$2"

  wallet_cli "$rpc_port" listdescriptors \
    | jq -e --arg descriptor "$descriptor" '.descriptors[] | select(.desc == $descriptor)' >/dev/null
}

ensure_signer_descriptor() {
  local node_name="$1"
  local rpc_port="$2"
  local descriptor="$3"
  local private_descriptor
  local public_descriptor
  local import_result

  private_descriptor="$(descriptor_with_private_checksum "$rpc_port" "$descriptor")"
  public_descriptor="$(descriptor_with_public_checksum "$rpc_port" "$descriptor")"

  if wallet_has_descriptor "$rpc_port" "$public_descriptor"; then
    return 0
  fi

  import_result="$(wallet_cli "$rpc_port" importdescriptors "[{\"desc\":\"$private_descriptor\",\"timestamp\":\"now\"}]")"

  if ! printf '%s' "$import_result" | jq -e 'length > 0 and .[0].success == true' >/dev/null; then
    printf '%s' "$import_result" | jq -r '.[0].error.message // "descriptor import failed"' >&2
    echo "Failed to import Signet signer descriptor into $node_name wallet" >&2
    exit 1
  fi
}

wait_for_rpc() {
  local node_name="$1"
  local rpc_port="$2"

  for ((attempt = 0; attempt < 60; attempt++)); do
    if bitcoin_cli "$rpc_port" getblockchaininfo >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  echo "$node_name RPC did not become ready in time" >&2
  exit 1
}

wait_for_peer_count() {
  local node_name="$1"
  local rpc_port="$2"
  local expected_peers="$3"
  local peer_count

  for ((attempt = 0; attempt < 60; attempt++)); do
    peer_count="$(bitcoin_cli "$rpc_port" getconnectioncount 2>/dev/null || echo 0)"
    if [[ "$peer_count" =~ ^[0-9]+$ ]] && (( peer_count == expected_peers )); then
      return 0
    fi
    echo "$node_name connections: $peer_count/$expected_peers"
    sleep 1
  done

  echo "$node_name did not reach $expected_peers peer connection(s) in time" >&2
  exit 1
}

start_bitcoind() {
  local data_dir="$1"
  local p2p_port="$2"
  local rpc_port="$3"
  local connect_p2p_port="${4:-}"
  local args=(
    -signet
    -signetchallenge="$SIGNET_CHALLENGE"
    -daemonwait
    -server=1
    -datadir="$data_dir"
    -port="$p2p_port"
    -rpcport="$rpc_port"
    -rpcbind="$LOCALHOST"
    -rpcallowip=127.0.0.1
    -rpcuser="$RPC_USER"
    -rpcpassword="$RPC_PASSWORD"
    -dnsseed=0
    -fixedseeds=0
    -listen=1
  )

  mkdir -p "$data_dir"

  if [[ "$SIGNET_ENABLE_DEFAULT_PEERS" == "1" && -n "$connect_p2p_port" ]]; then
    args+=(-connect="$LOCALHOST:$connect_p2p_port")
  fi

  bitcoind "${args[@]}"
}

print_status() {
  local node_name="$1"
  local rpc_port="$2"

  echo "$node_name chain: $(bitcoin_cli "$rpc_port" getblockchaininfo | jq -r '.chain')"
  echo "$node_name connections: $(bitcoin_cli "$rpc_port" getconnectioncount)"
}

main() {
  require_command bitcoind
  require_command bitcoin-cli
  require_command jq

  start_bitcoind "$OBSERVER_DATA_DIR" "$OBSERVER_P2P_PORT" "$OBSERVER_RPC_PORT"
  wait_for_rpc "Observer C" "$OBSERVER_RPC_PORT"

  start_bitcoind "$MINER_A_DATA_DIR" "$MINER_A_P2P_PORT" "$MINER_A_RPC_PORT" "$OBSERVER_P2P_PORT"
  start_bitcoind "$MINER_B_DATA_DIR" "$MINER_B_P2P_PORT" "$MINER_B_RPC_PORT" "$OBSERVER_P2P_PORT"

  echo "Waiting for Signet RPC..."
  wait_for_rpc "Miner A" "$MINER_A_RPC_PORT"
  wait_for_rpc "Miner B" "$MINER_B_RPC_PORT"

  echo "Preparing miner wallets..."
  ensure_miner_wallet "$MINER_A_RPC_PORT"
  ensure_miner_wallet "$MINER_B_RPC_PORT"
  ensure_signer_descriptor "Miner A" "$MINER_A_RPC_PORT" "$MINER_A_DESCRIPTOR"
  ensure_signer_descriptor "Miner B" "$MINER_B_RPC_PORT" "$MINER_B_DESCRIPTOR"

  if [[ "$SIGNET_ENABLE_DEFAULT_PEERS" == "1" ]]; then
    echo "Waiting for peer connections..."
    wait_for_peer_count "Miner A" "$MINER_A_RPC_PORT" 1
    wait_for_peer_count "Miner B" "$MINER_B_RPC_PORT" 1
    wait_for_peer_count "Observer C" "$OBSERVER_RPC_PORT" 2
  else
    echo "Signet nodes started without default peer links."
  fi

  print_status "Miner A" "$MINER_A_RPC_PORT"
  print_status "Miner B" "$MINER_B_RPC_PORT"
  print_status "Observer C" "$OBSERVER_RPC_PORT"
}

main "$@"
