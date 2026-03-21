use bitcoincore_rpc::bitcoin::{BlockHash, Network as BitcoinNetwork};
use serde::Serialize;
use std::fmt;

/// Selects whether a header should be fetched by height or by hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderLocator {
    /// Locates a header by chain height in the active chain.
    Height(u64),
    /// Locates a header by explicit block hash.
    Hash(BlockHash),
}

/// Stable metadata used to identify and describe a configured node.
#[derive(Debug, Hash, Clone, Eq, PartialEq)]
pub struct NodeInfo {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub implementation: String,
    pub network_type: BitcoinNetwork,
    pub supports_mining: bool,
    /// Custom signet challenge script (hex). Set from the network config.
    pub signet_challenge: Option<String>,
    /// Custom signet mining difficulty target (hex). Set from the network config.
    pub signet_nbits: Option<String>,
    /// P2P listening address (`host:port`) used for peer connections between nodes.
    /// Computed from `rpc_host` + `p2p_port` in the config; `None` when `p2p_port` is unset.
    pub p2p_address: Option<String>,
}

impl fmt::Display for NodeInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Node(id={}, name='{}', implementation='{}', network_type='{}', supports_mining={})",
            self.id, self.name, self.implementation, self.network_type, self.supports_mining
        )
    }
}

/// Peer connection information returned by `getpeerinfo`.
#[derive(Debug, Clone, Serialize)]
pub struct PeerInfo {
    pub id: u64,
    pub addr: String,
    /// Local bind address for this connection. For inbound peers this is the
    /// node's own listening address, which lets us discover the P2P port.
    #[serde(skip_serializing)]
    pub addrbind: String,
    pub subver: String,
    pub inbound: bool,
    pub connection_type: String,
    pub network: String,
}
