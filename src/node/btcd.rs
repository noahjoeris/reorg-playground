use crate::error::{FetchError, JsonRPCError};
use crate::node::shared_fetch;
use crate::node::{HeaderLocator, Node, NodeInfo};
use crate::types::{ChainTip, HeaderInfo, Tree};
use async_trait::async_trait;
use bitcoin_pool_identification::{PoolIdentification, default_data};
use bitcoincore_rpc::bitcoin;
use bitcoincore_rpc::bitcoin::BlockHash;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use tokio::task;

#[derive(Hash, Clone)]
pub struct BtcdNode {
    info: NodeInfo,
    rpc_endpoint: String,
    rpc_user: String,
    rpc_password: String,
}

impl BtcdNode {
    pub fn new(
        info: NodeInfo,
        rpc_endpoint: String,
        rpc_user: String,
        rpc_password: String,
    ) -> Self {
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
}

#[async_trait]
impl Node for BtcdNode {
    fn info(&self) -> &NodeInfo {
        &self.info
    }

    fn endpoint(&self) -> &str {
        &self.rpc_endpoint
    }

    async fn version(&self) -> Result<String, FetchError> {
        Err(FetchError::BtcdRPC(JsonRPCError::NotImplemented))
    }

    async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError> {
        let url = self.rpc_url();
        let user = self.rpc_user.clone();
        let password = self.rpc_password.clone();

        match locator {
            HeaderLocator::Hash(hash) => {
                task::spawn_blocking(move || {
                    crate::jsonrpc::btcd_blockheader(&url, &user, &password, &hash.to_string())
                        .map_err(FetchError::BtcdRPC)
                })
                .await?
            }
            HeaderLocator::Height(height) => {
                task::spawn_blocking(move || {
                    let hash = crate::jsonrpc::btcd_blockhash(&url, &user, &password, height)
                        .map_err(FetchError::BtcdRPC)?;
                    crate::jsonrpc::btcd_blockheader(&url, &user, &password, &hash.to_string())
                        .map_err(FetchError::BtcdRPC)
                })
                .await?
            }
        }
    }

    async fn get_miner_pool(
        &self,
        hash: &BlockHash,
        _height: u64,
        network: bitcoin::Network,
    ) -> Result<Option<String>, FetchError> {
        let hash = *hash;
        let url = self.rpc_url();
        let user = self.rpc_user.clone();
        let password = self.rpc_password.clone();

        let coinbase =
            task::spawn_blocking(move || {
                let block = crate::jsonrpc::btcd_block(&url, &user, &password, &hash.to_string())
                    .map_err(FetchError::BtcdRPC)?;

                block.txdata.into_iter().next().ok_or_else(|| {
                    FetchError::DataError(format!("Block {} has no transactions", hash))
                })
            })
            .await??;

        let miner_identification_data = default_data(network);
        Ok(coinbase
            .identify_pool(network, &miner_identification_data)
            .map(|result| result.pool.name))
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

    async fn get_new_headers(
        &self,
        tips: &[ChainTip],
        tree: &Tree,
        first_tracked_height: u64,
        progress_tx: Option<&tokio::sync::mpsc::UnboundedSender<Vec<HeaderInfo>>>,
    ) -> Result<(Vec<HeaderInfo>, Vec<BlockHash>), FetchError> {
        let mut active_new_headers = shared_fetch::get_new_active_headers_by_height(
            self,
            tips,
            tree,
            first_tracked_height,
            progress_tx,
        )
        .await?;
        let mut nonactive_new_headers = shared_fetch::get_new_nonactive_headers_by_hash(
            self,
            tips,
            tree,
            first_tracked_height,
            progress_tx,
        )
        .await?;

        let headers_needing_miners =
            shared_fetch::miner_hashes_for_new_headers(&active_new_headers, &nonactive_new_headers);
        active_new_headers.append(&mut nonactive_new_headers);
        Ok((active_new_headers, headers_needing_miners))
    }
}
