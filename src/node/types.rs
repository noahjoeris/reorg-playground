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

/// Describes which fetch operations a backend implementation supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// True when header lookup by block hash is supported.
    pub supports_hash_header_lookup: bool,
    /// True when header lookup by chain height is supported.
    pub supports_height_header_lookup: bool,
    /// True when bulk active-chain header retrieval is supported.
    pub supports_batch_active_headers: bool,
    /// True when non-active branch header traversal is supported.
    pub supports_nonactive_headers: bool,
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
