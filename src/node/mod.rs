//! Node backend abstraction and shared entry points for header retrieval.

mod bitcoin_core;
mod btcd;
mod electrum;
mod esplora;
mod shared_fetch;
mod signet_mining;
mod types;

use crate::error::FetchError;
use crate::types::{ChainTip, HeaderInfo, Tree};
use async_trait::async_trait;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use bitcoincore_rpc::bitcoin::{BlockHash, Network as BitcoinNetwork};
use tokio::sync::mpsc::UnboundedSender;

pub use bitcoin_core::BitcoinCoreNode;
pub use btcd::BtcdNode;
pub use electrum::Electrum;
pub use esplora::Esplora;
pub use types::{HeaderLocator, NodeInfo};

fn is_regtest_or_signet(network: BitcoinNetwork) -> bool {
    matches!(network, BitcoinNetwork::Regtest | BitcoinNetwork::Signet)
}

/// Backend interface for fetching active-chain headers in batches usually via REST API.
#[async_trait]
pub(crate) trait ActiveHeadersBatchProvider: Send + Sync {
    async fn batch_active_headers(
        &self,
        start_height: u64,
        count: u64,
    ) -> Result<Vec<Header>, FetchError>;
}

/// Common interface implemented by all supported backend node clients.
#[async_trait]
pub trait Node: Send + Sync {
    fn info(&self) -> &NodeInfo;
    fn endpoint(&self) -> &str;

    fn supports_controls(&self, disable_node_controls: bool) -> bool {
        !disable_node_controls
            && self.info().implementation == "Bitcoin Core"
            && is_regtest_or_signet(self.info().network_type)
    }

    fn supports_mining(&self, disable_node_controls: bool) -> bool {
        self.supports_controls(disable_node_controls) && self.info().supports_mining
    }

    async fn version(&self) -> Result<String, FetchError>;
    /// Fetches a header by hash or by height, depending on the provided locator.
    async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError>;
    /// Returns chain tip information visible to this backend.
    async fn tips(&self) -> Result<Vec<ChainTip>, FetchError>;
    /// Identifies the miner pool for the given block, if possible.
    async fn get_miner_pool(
        &self,
        hash: &BlockHash,
        height: u64,
        network: BitcoinNetwork,
    ) -> Result<Option<String>, FetchError>;

    /// Loads new active/non-active headers and returns hashes that still need miner identification.
    async fn get_new_headers(
        &self,
        tips: &[ChainTip],
        tree: &Tree,
        first_tracked_height: u64,
        progress_tx: Option<&UnboundedSender<Vec<HeaderInfo>>>,
    ) -> Result<(Vec<HeaderInfo>, Vec<BlockHash>), FetchError>;

    /// Mines new blocks when supported by the backend/network.
    async fn mine_new_blocks(&self, _count: u64) -> Result<Vec<BlockHash>, FetchError> {
        Err(FetchError::NotSupported {
            node: self.info().implementation.clone(),
            operation: "mine_new_blocks",
        })
    }

    /// Returns whether P2P networking is currently active when supported by the backend.
    async fn p2p_network_active(&self) -> Result<bool, FetchError> {
        Err(FetchError::NotSupported {
            node: self.info().implementation.clone(),
            operation: "p2p_network_active",
        })
    }

    /// Toggles P2P network activity when supported by the backend.
    async fn set_p2p_network_active(&self, _active: bool) -> Result<(), FetchError> {
        Err(FetchError::NotSupported {
            node: self.info().implementation.clone(),
            operation: "set_p2p_network_active",
        })
    }
}
