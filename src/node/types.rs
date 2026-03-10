use bitcoincore_rpc::bitcoin::{BlockHash, Network as BitcoinNetwork};
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
