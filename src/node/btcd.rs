use crate::error::{FetchError, JsonRPCError};
use crate::node::{Capabilities, HeaderLocator, Node, NodeInfo};
use crate::types::ChainTip;
use async_trait::async_trait;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use bitcoincore_rpc::bitcoin::{BlockHash, Transaction};
use tokio::task;

#[derive(Hash, Clone)]
pub struct BtcdNode {
    info: NodeInfo,
    rpc_endpoint: String,
    rpc_user: String,
    rpc_password: String,
}

impl BtcdNode {
    pub fn new(info: NodeInfo, rpc_endpoint: String, rpc_user: String, rpc_password: String) -> Self {
        BtcdNode {
            info,
            rpc_endpoint,
            rpc_user,
            rpc_password,
        }
    }

    fn rpc_url(&self) -> String {
        format!("http://{}/", self.rpc_endpoint)
    }

    fn not_supported(&self, operation: &'static str) -> FetchError {
        FetchError::NotSupported {
            node: self.info.implementation.clone(),
            operation,
        }
    }
}

#[async_trait]
impl Node for BtcdNode {
    fn info(&self) -> &NodeInfo {
        &self.info
    }

    fn endpoint(&self) -> &str {
        &self.rpc_endpoint
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_hash_header_lookup: true,
            supports_height_header_lookup: false,
            supports_batch_active_headers: false,
            supports_nonactive_headers: true,
        }
    }

    async fn version(&self) -> Result<String, FetchError> {
        Err(FetchError::BtcdRPC(JsonRPCError::NotImplemented))
    }

    async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError> {
        let hash = match locator {
            HeaderLocator::Hash(hash) => hash,
            HeaderLocator::Height(_) => return Err(self.not_supported("block_header(height)")),
        };

        let url = self.rpc_url();
        let user = self.rpc_user.clone();
        let password = self.rpc_password.clone();

        task::spawn_blocking(move || {
            crate::jsonrpc::btcd_blockheader(&url, &user, &password, &hash.to_string())
                .map_err(FetchError::BtcdRPC)
        })
        .await?
    }

    async fn batch_active_headers(
        &self,
        _start_height: u64,
        _count: u64,
    ) -> Result<Vec<Header>, FetchError> {
        Err(self.not_supported("batch_active_headers"))
    }

    async fn coinbase(&self, hash: &BlockHash, _height: u64) -> Result<Transaction, FetchError> {
        let hash = *hash;
        let url = self.rpc_url();
        let user = self.rpc_user.clone();
        let password = self.rpc_password.clone();

        task::spawn_blocking(move || {
            let block = crate::jsonrpc::btcd_block(&url, &user, &password, &hash.to_string())
                .map_err(FetchError::BtcdRPC)?;

            block.txdata.into_iter().next().ok_or_else(|| {
                FetchError::DataError(format!("Block {} has no transactions", hash))
            })
        })
        .await?
    }

    async fn block_hash(&self, height: u64) -> Result<BlockHash, FetchError> {
        let url = self.rpc_url();
        let user = self.rpc_user.clone();
        let password = self.rpc_password.clone();

        task::spawn_blocking(move || {
            crate::jsonrpc::btcd_blockhash(&url, &user, &password, height)
                .map_err(FetchError::BtcdRPC)
        })
        .await?
    }

    async fn tips(&self) -> Result<Vec<ChainTip>, FetchError> {
        let url = self.rpc_url();
        let user = self.rpc_user.clone();
        let password = self.rpc_password.clone();

        task::spawn_blocking(move || {
            crate::jsonrpc::btcd_chaintips(&url, &user, &password).map_err(FetchError::BtcdRPC)
        })
        .await?
    }
}
