#!/usr/bin/env bash
set -euo pipefail

# Shared settings
RPC_HOST="127.0.0.1"
RPC_USER="reorg-playground"
RPC_PASSWORD="reorg-playground"

# Node A
NODE_A_DATA_DIR="$HOME/Library/Application Support/Bitcoin/regtest-nodeA" # Adapt to your system
NODE_A_RPC_PORT="18443"

# Node B
NODE_B_DATA_DIR="$HOME/Library/Application Support/Bitcoin/regtest-nodeB" # Adapt to your system
NODE_B_RPC_PORT="18453"

stop_node() {
  local node_name="$1"
  local node_data_dir="$2"
  local node_rpc_port="$3"
  local output=""

  if output=$(bitcoin-cli -regtest \
    -datadir="$node_data_dir" \
    -rpcconnect="$RPC_HOST" \
    -rpcport="$node_rpc_port" \
    -rpcuser="$RPC_USER" \
    -rpcpassword="$RPC_PASSWORD" \
    stop 2>&1); then
    echo "$node_name: $output"
  else
    echo "$node_name: already stopped or RPC unavailable"
  fi
}

stop_node "Node A" "$NODE_A_DATA_DIR" "$NODE_A_RPC_PORT"
stop_node "Node B" "$NODE_B_DATA_DIR" "$NODE_B_RPC_PORT"
