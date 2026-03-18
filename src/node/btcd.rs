use crate::error::{FetchError, JsonRPCError};
use crate::node::shared_fetch::{self, RpcAuth, jsonrpc_call};
use crate::node::{HeaderLocator, Node, NodeInfo};
use crate::types::{ChainTip, HeaderInfo, Tree};
use async_trait::async_trait;
use bitcoin_pool_identification::{PoolIdentification, default_data};
use bitcoincore_rpc::bitcoin;
use bitcoincore_rpc::bitcoin::Block;
use bitcoincore_rpc::bitcoin::BlockHash;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use serde_json::Value;
use std::str::FromStr;
use tokio::task;

const BITCOIN_BLOCK_HEADER_HEX_LENGTH: usize = 80 * 2;
const BITCOIN_BLOCK_HASH_HEX_LENGTH: usize = 32 * 2;

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

    fn rpc_auth(&self) -> RpcAuth {
        RpcAuth {
            url: format!("http://{}/", self.rpc_endpoint),
            user: self.rpc_user.clone(),
            password: self.rpc_password.clone(),
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

    fn supports_stale_tips(&self) -> bool {
        true
    }

    async fn version(&self) -> Result<String, FetchError> {
        Err(FetchError::BtcdRPC(JsonRPCError::NotImplemented))
    }

    async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError> {
        let auth = self.rpc_auth();

        task::spawn_blocking(move || {
            let hash_str = match locator {
                HeaderLocator::Hash(hash) => hash.to_string(),
                HeaderLocator::Height(height) => {
                    let hash_hex: String =
                        jsonrpc_call("getblockhash", vec![Value::from(height)], &auth)
                            .map_err(FetchError::BtcdRPC)?
                            .unwrap_or_default();
                    if hash_hex.len() != BITCOIN_BLOCK_HASH_HEX_LENGTH {
                        return Err(FetchError::BtcdRPC(
                            JsonRPCError::RpcUnexpectedResponseContents(format!(
                                "getblockhash: expected {} hex chars but got {}: {}",
                                BITCOIN_BLOCK_HASH_HEX_LENGTH,
                                hash_hex.len(),
                                hash_hex
                            )),
                        ));
                    }
                    hash_hex
                }
            };

            let header_hex: String = jsonrpc_call(
                "getblockheader",
                vec![Value::from(hash_str.as_str()), Value::from(false)],
                &auth,
            )
            .map_err(FetchError::BtcdRPC)?
            .unwrap_or_default();
            if header_hex.len() != BITCOIN_BLOCK_HEADER_HEX_LENGTH {
                return Err(FetchError::BtcdRPC(
                    JsonRPCError::RpcUnexpectedResponseContents(format!(
                        "getblockheader: expected {} hex chars but got {}: {}",
                        BITCOIN_BLOCK_HEADER_HEX_LENGTH,
                        header_hex.len(),
                        header_hex
                    )),
                ));
            }
            let header_bytes =
                hex::decode(header_hex).map_err(|e| FetchError::BtcdRPC(e.into()))?;
            let header: Header = bitcoin::consensus::deserialize(&header_bytes)
                .map_err(|e| FetchError::BtcdRPC(e.into()))?;
            Ok(header)
        })
        .await?
    }

    async fn get_miner_pool(
        &self,
        hash: &BlockHash,
        _height: u64,
        network: bitcoin::Network,
    ) -> Result<Option<String>, FetchError> {
        let hash = *hash;
        let auth = self.rpc_auth();

        let coinbase = task::spawn_blocking(move || {
            let hash_str = hash.to_string();
            let block_hex: String = jsonrpc_call(
                "getblock",
                vec![Value::from(hash_str.as_str()), Value::from(0i8)],
                &auth,
            )
            .map_err(FetchError::BtcdRPC)?
            .unwrap_or_default();
            let block_bytes = hex::decode(block_hex).map_err(|e| FetchError::BtcdRPC(e.into()))?;
            let block: Block = bitcoin::consensus::deserialize(&block_bytes)
                .map_err(|e| FetchError::BtcdRPC(e.into()))?;

            block
                .txdata
                .into_iter()
                .next()
                .ok_or_else(|| FetchError::DataError(format!("Block {} has no transactions", hash)))
        })
        .await??;

        let miner_identification_data = default_data(network);
        Ok(coinbase
            .identify_pool(network, &miner_identification_data)
            .map(|result| result.pool.name))
    }

    async fn tips(&self) -> Result<Vec<ChainTip>, FetchError> {
        let auth = self.rpc_auth();

        task::spawn_blocking(move || {
            jsonrpc_call::<Vec<ChainTip>>("getchaintips", vec![], &auth)
                .map_err(FetchError::BtcdRPC)?
                .ok_or_else(|| {
                    FetchError::BtcdRPC(JsonRPCError::JsonRpc(
                        "getchaintips response was empty".to_string(),
                    ))
                })
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

    async fn mine_new_blocks(&self, count: u64) -> Result<Vec<BlockHash>, FetchError> {
        if count == 0 {
            return Err(FetchError::DataError(
                "mine_new_blocks requires count > 0".to_string(),
            ));
        }
        if self.info.network_type != bitcoin::Network::Regtest {
            return Err(FetchError::NotSupported {
                node: self.info.implementation.clone(),
                operation: "mine_new_blocks",
            });
        }

        let auth = self.rpc_auth();
        task::spawn_blocking(move || {
            let hashes: Vec<String> = jsonrpc_call("generate", vec![Value::from(count)], &auth)
                .map_err(FetchError::BtcdRPC)?
                .ok_or_else(|| {
                    FetchError::BtcdRPC(JsonRPCError::JsonRpc(
                        "generate response was empty".to_string(),
                    ))
                })?;

            hashes
                .into_iter()
                .map(|hash| BlockHash::from_str(&hash).map_err(|e| FetchError::BtcdRPC(e.into())))
                .collect()
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_node(network_type: bitcoin::Network) -> BtcdNode {
        BtcdNode::new(
            NodeInfo {
                id: 1,
                name: "test".to_string(),
                description: "test node".to_string(),
                implementation: "btcd".to_string(),
                network_type,
                supports_mining: true,
                signet_challenge: None,
                signet_nbits: None,
            },
            "127.0.0.1:18334".to_string(),
            "user".to_string(),
            "pass".to_string(),
        )
    }

    #[tokio::test]
    async fn mine_new_blocks_rejects_zero_count() {
        let node = test_node(bitcoin::Network::Regtest);
        let result = node.mine_new_blocks(0).await;
        assert!(matches!(result, Err(FetchError::DataError(_))));
    }
}
