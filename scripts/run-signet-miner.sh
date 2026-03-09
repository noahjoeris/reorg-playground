#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

RPC_USER="${SIGNET_RPC_USER:-reorg-playground}"
RPC_PASSWORD="${SIGNET_RPC_PASSWORD:-reorg-playground}"
RPC_TIMEOUT=10
MINER_WALLET="miner"

SIGNET_CHALLENGE="5121031b14827738eaf41b67f50a2ddd9d0b08907236f3d0e79bef7fc9ea7b866a3ca821036739e2fc3681d2ef7f9afc0bb2f8964c4f61b6c30f4642cc0322e57967d31f3652ae"
SIGNET_NBITS="1e0377ae"
MINER_A_DESCRIPTOR="multi(1,cUziKDUPDUDf8XUb9MURQ15Nj71poddNYY4XvUnL4wDqXD2Tj26A,036739e2fc3681d2ef7f9afc0bb2f8964c4f61b6c30f4642cc0322e57967d31f36)"
MINER_B_DESCRIPTOR="multi(1,031b14827738eaf41b67f50a2ddd9d0b08907236f3d0e79bef7fc9ea7b866a3ca8,cV5hR5xMAnzQXrSvs6X6CKt45LrRrtkoJyEfynumw6hK73ivZETk)"

BITCOIN_CORE_SIGNET_MINER="${BITCOIN_CORE_SIGNET_MINER:-$REPO_ROOT/bitcoin/contrib/signet/miner}"
BITCOIN_UTIL="${BITCOIN_UTIL:-bitcoin-util}"
MINER_A_RPC_HOST="${SIGNET_NODE_A_RPC_HOST:-127.0.0.1}"
MINER_A_RPC_PORT="${SIGNET_NODE_A_RPC_PORT:-38332}"
MINER_B_RPC_HOST="${SIGNET_NODE_B_RPC_HOST:-127.0.0.1}"
MINER_B_RPC_PORT="${SIGNET_NODE_B_RPC_PORT:-39332}"
OBSERVER_RPC_HOST="${SIGNET_NODE_C_RPC_HOST:-127.0.0.1}"
OBSERVER_RPC_PORT="${SIGNET_NODE_C_RPC_PORT:-40332}"

TARGET=""
BLOCK_COUNT=1
TARGET_NAME=""
TARGET_RPC_HOST=""
TARGET_RPC_PORT=""
TARGET_SIGNER_DESCRIPTOR=""

usage() {
  echo "Usage: $0 <a|b|c> [count]" >&2
  exit 1
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

select_target() {
  case "$TARGET" in
    a|A)
      TARGET_NAME="Miner A"
      TARGET_RPC_HOST="$MINER_A_RPC_HOST"
      TARGET_RPC_PORT="$MINER_A_RPC_PORT"
      TARGET_SIGNER_DESCRIPTOR="$MINER_A_DESCRIPTOR"
      ;;
    b|B)
      TARGET_NAME="Miner B"
      TARGET_RPC_HOST="$MINER_B_RPC_HOST"
      TARGET_RPC_PORT="$MINER_B_RPC_PORT"
      TARGET_SIGNER_DESCRIPTOR="$MINER_B_DESCRIPTOR"
      ;;
    c|C)
      TARGET_NAME="Observer C"
      TARGET_RPC_HOST="$OBSERVER_RPC_HOST"
      TARGET_RPC_PORT="$OBSERVER_RPC_PORT"
      TARGET_SIGNER_DESCRIPTOR=""
      ;;
    *)
      usage
      ;;
  esac
}

bitcoin_cli() {
  bitcoin-cli \
    -signet \
    -signetchallenge="$SIGNET_CHALLENGE" \
    -rpcclienttimeout="$RPC_TIMEOUT" \
    -rpcconnect="$TARGET_RPC_HOST" \
    -rpcport="$TARGET_RPC_PORT" \
    -rpcuser="$RPC_USER" \
    -rpcpassword="$RPC_PASSWORD" \
    "$@"
}

wallet_cli() {
  bitcoin_cli -rpcwallet="$MINER_WALLET" "$@"
}

miner_cli_command() {
  local joined=""
  local arg
  local args=(
    bitcoin-cli
    "-signetchallenge=$SIGNET_CHALLENGE"
    "-rpcclienttimeout=$RPC_TIMEOUT"
    "-rpcconnect=$TARGET_RPC_HOST"
    "-rpcport=$TARGET_RPC_PORT"
    "-rpcuser=$RPC_USER"
    "-rpcpassword=$RPC_PASSWORD"
    "-rpcwallet=$MINER_WALLET"
  )

  for arg in "${args[@]}"; do
    printf -v joined '%s%q ' "$joined" "$arg"
  done

  printf '%s' "${joined% }"
}

descriptor_with_private_checksum() {
  local descriptor="$1"
  local checksum

  checksum="$(bitcoin_cli getdescriptorinfo "$descriptor" | jq -er '.checksum')"
  printf '%s#%s\n' "$descriptor" "$checksum"
}

descriptor_with_public_checksum() {
  local descriptor="$1"
  local descriptor_info

  descriptor_info="$(bitcoin_cli getdescriptorinfo "$descriptor")"
  printf '%s#%s\n' \
    "$(printf '%s' "$descriptor_info" | jq -er '.descriptor')" \
    "$(printf '%s' "$descriptor_info" | jq -er '.checksum')"
}

wallet_has_descriptor() {
  local descriptor="$1"

  wallet_cli listdescriptors \
    | jq -e --arg descriptor "$descriptor" '.descriptors[] | select(.desc == $descriptor)' >/dev/null
}

ensure_miner_wallet() {
  if bitcoin_cli listwallets | jq -e --arg wallet "$MINER_WALLET" 'index($wallet) != null' >/dev/null; then
    return 0
  fi

  if bitcoin_cli loadwallet "$MINER_WALLET" >/dev/null 2>&1; then
    return 0
  fi

  bitcoin_cli createwallet "$MINER_WALLET" >/dev/null
}

ensure_signer_descriptor() {
  local private_descriptor
  local public_descriptor
  local import_result

  private_descriptor="$(descriptor_with_private_checksum "$TARGET_SIGNER_DESCRIPTOR")"
  public_descriptor="$(descriptor_with_public_checksum "$TARGET_SIGNER_DESCRIPTOR")"

  if wallet_has_descriptor "$public_descriptor"; then
    return 0
  fi

  import_result="$(wallet_cli importdescriptors "[{\"desc\":\"$private_descriptor\",\"timestamp\":\"now\"}]")"

  if ! printf '%s' "$import_result" | jq -e 'length > 0 and .[0].success == true' >/dev/null; then
    printf '%s' "$import_result" | jq -r '.[0].error.message // "descriptor import failed"' >&2
    echo "Failed to import Signet signer descriptor into $TARGET_NAME wallet" >&2
    exit 1
  fi
}

ensure_miner_script() {
  if [[ ! -f "$BITCOIN_CORE_SIGNET_MINER" ]]; then
    echo "Missing Bitcoin Core signet miner script at $BITCOIN_CORE_SIGNET_MINER" >&2
    echo "Set BITCOIN_CORE_SIGNET_MINER to a Bitcoin Core contrib/signet/miner path." >&2
    exit 1
  fi
}

validated_block_time() {
  local template
  local challenge
  local mintime
  local now
  local wait_seconds

  template="$(bitcoin_cli getblocktemplate '{"rules":["signet","segwit"]}')"
  challenge="$(printf '%s' "$template" | jq -er '.signet_challenge')"

  if [[ "$challenge" != "$SIGNET_CHALLENGE" ]]; then
    echo "$TARGET_NAME returned unexpected signet_challenge: $challenge" >&2
    echo "Expected: $SIGNET_CHALLENGE" >&2
    exit 1
  fi

  mintime="$(printf '%s' "$template" | jq -er '.mintime')"
  now="$(date +%s)"

  if (( mintime >= now )); then
    wait_seconds=$((mintime - now + 1))
    sleep "$wait_seconds"
  fi

  printf '%s\n' "$mintime"
}

mine_one_block() {
  local reward_address="$1"
  local best_hash_before
  local best_hash_after
  local block_time
  local miner_output

  best_hash_before="$(bitcoin_cli getbestblockhash)"
  block_time="$(validated_block_time)"

  miner_output="$(
    python3 "$BITCOIN_CORE_SIGNET_MINER" \
      --cli="$(miner_cli_command)" \
      --quiet \
      generate \
      --set-block-time="$block_time" \
      --grind-cmd="$BITCOIN_UTIL grind" \
      --address="$reward_address" \
      --nbits="$SIGNET_NBITS" \
      2>&1
  )"

  if [[ -n "$miner_output" ]]; then
    printf '%s\n' "$miner_output" >&2
    return 1
  fi

  best_hash_after="$(bitcoin_cli getbestblockhash)"
  if [[ "$best_hash_after" == "$best_hash_before" ]]; then
    echo "Mining did not advance $TARGET_NAME to a new best block" >&2
    return 1
  fi

  printf '%s\n' "$best_hash_after"
}

main() {
  TARGET="${1:-}"
  BLOCK_COUNT="${2:-1}"

  if [[ -z "$TARGET" ]]; then
    usage
  fi

  if ! [[ "$BLOCK_COUNT" =~ ^[1-9][0-9]*$ ]]; then
    echo "count must be a positive integer" >&2
    exit 1
  fi

  require_command bitcoin-cli
  require_command "$BITCOIN_UTIL"
  require_command jq
  require_command python3

  select_target
  ensure_miner_script

  bitcoin_cli getblockchaininfo >/dev/null
  ensure_miner_wallet
  if [[ -n "$TARGET_SIGNER_DESCRIPTOR" ]]; then
    ensure_signer_descriptor
  fi

  for ((block_index = 0; block_index < BLOCK_COUNT; block_index++)); do
    reward_address="$(wallet_cli getnewaddress)"
    best_hash="$(mine_one_block "$reward_address")"
    echo "Mined 1 block on $TARGET_NAME to $reward_address"
    echo "$best_hash"
  done
}

main "$@"
