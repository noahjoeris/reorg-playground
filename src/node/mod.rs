//! Node backend abstraction and shared entry points for header retrieval.

mod bitcoin_core;
mod btcd;
mod electrum;
mod esplora;
mod shared_fetch;
mod types;

use crate::error::FetchError;
use crate::types::{ChainTip, HeaderInfo, Tree};
use async_trait::async_trait;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use bitcoincore_rpc::bitcoin::{BlockHash, Transaction};
use tokio::sync::mpsc::UnboundedSender;

pub use bitcoin_core::BitcoinCoreNode;
pub use btcd::BtcdNode;
pub use electrum::Electrum;
pub use esplora::Esplora;
pub use types::{Capabilities, HeaderLocator, NodeInfo};

/// Common interface implemented by all supported backend node clients.
#[async_trait]
pub trait Node: Send + Sync {
    fn info(&self) -> &NodeInfo;
    fn endpoint(&self) -> &str;
    /// Advertises backend feature support so callers can choose valid fetch paths.
    fn capabilities(&self) -> Capabilities;

    async fn version(&self) -> Result<String, FetchError>;
    async fn block_hash(&self, height: u64) -> Result<BlockHash, FetchError>;
    /// Fetches a header by hash or by height, depending on the provided locator.
    async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError>;
    /// Fetches up to `count` sequential active-chain headers from `start_height`.
    async fn batch_active_headers(
        &self,
        start_height: u64,
        count: u64,
    ) -> Result<Vec<Header>, FetchError>;
    /// Returns chain tip information visible to this backend.
    async fn tips(&self) -> Result<Vec<ChainTip>, FetchError>;
    /// Fetches a block's coinbase transaction; used to identify miners.
    async fn coinbase(&self, hash: &BlockHash, height: u64) -> Result<Transaction, FetchError>;

    /// Loads new active/non-active headers and returns hashes that still need miner identification.
    async fn new_headers(
        &self,
        tips: &[ChainTip],
        tree: &Tree,
        first_tracked_height: u64,
        progress_tx: Option<&UnboundedSender<Vec<HeaderInfo>>>,
    ) -> Result<(Vec<HeaderInfo>, Vec<BlockHash>), FetchError> {
        shared_fetch::new_headers(self, tips, tree, first_tracked_height, progress_tx).await
    }
}
