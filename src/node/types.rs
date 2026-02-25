use bitcoincore_rpc::bitcoin::BlockHash;
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
}

impl fmt::Display for NodeInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Node(id={}, name='{}', implementation='{}')",
            self.id, self.name, self.implementation
        )
    }
}
