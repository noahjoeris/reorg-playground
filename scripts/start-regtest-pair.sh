#!/usr/bin/env bash
set -euo pipefail

# Shared settings
RPC_HOST="127.0.0.1"
RPC_USER="reorg-playground"
RPC_PASSWORD="reorg-playground"

# Node A
NODE_A_DATA_DIR="$HOME/Library/Application Support/Bitcoin/regtest-nodeA"
NODE_A_P2P_PORT="18444"
NODE_A_RPC_PORT="18443"

# Node B
NODE_B_DATA_DIR="$HOME/Library/Application Support/Bitcoin/regtest-nodeB"
NODE_B_P2P_PORT="18454"
NODE_B_RPC_PORT="18453"

mkdir -p "$NODE_A_DATA_DIR" "$NODE_B_DATA_DIR"

bitcoind -regtest -daemon -datadir="$NODE_A_DATA_DIR" -port="$NODE_A_P2P_PORT" -rpcport="$NODE_A_RPC_PORT" -rpcbind="$RPC_HOST" -rpcallowip=127.0.0.1 -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASSWORD"
bitcoind -regtest -daemon -datadir="$NODE_B_DATA_DIR" -port="$NODE_B_P2P_PORT" -rpcport="$NODE_B_RPC_PORT" -rpcbind="$RPC_HOST" -rpcallowip=127.0.0.1 -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASSWORD"

sleep 5

bitcoin-cli -regtest -datadir="$NODE_A_DATA_DIR" -rpcconnect="$RPC_HOST" -rpcport="$NODE_A_RPC_PORT" -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASSWORD" addnode "${RPC_HOST}:${NODE_B_P2P_PORT}" onetry
bitcoin-cli -regtest -datadir="$NODE_B_DATA_DIR" -rpcconnect="$RPC_HOST" -rpcport="$NODE_B_RPC_PORT" -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASSWORD" addnode "${RPC_HOST}:${NODE_A_P2P_PORT}" onetry

echo "Node A connections: $(bitcoin-cli -regtest -datadir="$NODE_A_DATA_DIR" -rpcconnect="$RPC_HOST" -rpcport="$NODE_A_RPC_PORT" -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASSWORD" getconnectioncount)"
echo "Node B connections: $(bitcoin-cli -regtest -datadir="$NODE_B_DATA_DIR" -rpcconnect="$RPC_HOST" -rpcport="$NODE_B_RPC_PORT" -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASSWORD" getconnectioncount)"
