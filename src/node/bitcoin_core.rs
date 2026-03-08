use crate::error::FetchError;
use crate::node::shared_fetch;
use crate::node::{ActiveHeadersBatchProvider, HeaderLocator, Node, NodeInfo};
use crate::types::{ChainTip, HeaderInfo, Tree};
use async_trait::async_trait;
use bitcoin_pool_identification::{PoolIdentification, default_data};
use bitcoincore_rpc::bitcoin;
use bitcoincore_rpc::bitcoin::BlockHash;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use bitcoincore_rpc::jsonrpc;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use log::debug;
use tokio::task;

const MINER_WALLET: &str = "miner";

#[derive(Hash, Clone)]
pub struct BitcoinCoreNode {
    info: NodeInfo,
    rpc_endpoint: String,
    rpc_auth: Auth,
    use_rest: bool,
}

impl BitcoinCoreNode {
    pub fn new(info: NodeInfo, rpc_endpoint: String, rpc_auth: Auth, use_rest: bool) -> Self {
        BitcoinCoreNode {
            info,
            rpc_endpoint,
            rpc_auth,
            use_rest,
        }
    }

    fn rpc_client_with_url(&self, rpc_url: &str) -> Result<Client, FetchError> {
        let (user, pass) = self.rpc_auth.clone().get_user_pass()?;

        let mut transport_builder = jsonrpc::minreq_http::MinreqHttpTransport::builder()
            .url(rpc_url)
            .map_err(|e| {
                FetchError::DataError(format!(
                    "Could not set RPC URL '{}' for node {}: {}",
                    rpc_url,
                    self.info(),
                    e
                ))
            })?;

        if let Some(user) = user {
            transport_builder = transport_builder.basic_auth(user, pass);
        }

        Ok(Client::from_jsonrpc(jsonrpc::Client::with_transport(
            transport_builder.build(),
        )))
    }

    fn rpc_client(&self) -> Result<Client, FetchError> {
        self.rpc_client_with_url(&self.normalized_rpc_url())
    }

    fn wallet_rpc_url(&self, wallet: &str) -> String {
        format!(
            "{}/wallet/{}",
            self.normalized_rpc_url().trim_end_matches('/'),
            wallet
        )
    }

    fn normalized_rpc_url(&self) -> String {
        if self.rpc_endpoint.contains("://") {
            self.rpc_endpoint.clone()
        } else {
            format!("http://{}", self.rpc_endpoint)
        }
    }

    async fn with_rpc<T, F>(&self, op: F) -> Result<T, FetchError>
    where
        T: Send + 'static,
        F: FnOnce(Client) -> Result<T, bitcoincore_rpc::Error> + Send + 'static,
    {
        let rpc = self.rpc_client()?;
        let result = task::spawn_blocking(move || op(rpc)).await?;
        result.map_err(FetchError::from)
    }

    async fn with_wallet_rpc<T, F>(&self, wallet: &str, op: F) -> Result<T, FetchError>
    where
        T: Send + 'static,
        F: FnOnce(Client) -> Result<T, bitcoincore_rpc::Error> + Send + 'static,
    {
        let rpc = self.rpc_client_with_url(&self.wallet_rpc_url(wallet))?;
        let result = task::spawn_blocking(move || op(rpc)).await?;
        result.map_err(FetchError::from)
    }

    fn not_supported(&self, operation: &'static str) -> FetchError {
        FetchError::NotSupported {
            node: self.info.implementation.clone(),
            operation,
        }
    }

    async fn ensure_wallet_loaded(&self, wallet: &str) -> Result<(), FetchError> {
        let wallet_name = wallet.to_string();
        let loaded_wallets = self.with_rpc(|rpc| rpc.list_wallets()).await?;
        if loaded_wallets.iter().any(|w| w == &wallet_name) {
            return Ok(());
        }

        let wallet_for_load = wallet_name.clone();
        if self
            .with_rpc(move |rpc| rpc.load_wallet(&wallet_for_load).map(|_| ()))
            .await
            .is_ok()
        {
            return Ok(());
        }

        let wallet_for_create = wallet_name.clone();
        self.with_rpc(move |rpc| {
            rpc.create_wallet(&wallet_for_create, None, None, None, None)
                .map(|_| ())
        })
        .await?;
        Ok(())
    }
}

#[async_trait]
impl ActiveHeadersBatchProvider for BitcoinCoreNode {
    async fn batch_active_headers(
        &self,
        start_height: u64,
        count: u64,
    ) -> Result<Vec<Header>, FetchError> {
        if !self.use_rest {
            return Err(self.not_supported("batch_active_headers"));
        }

        let start_hash = self
            .with_rpc(move |rpc| rpc.get_block_hash(start_height))
            .await?;
        debug!(
            "loading active-chain headers starting from {} ({})",
            start_height, start_hash
        );

        let base_url = self.normalized_rpc_url().trim_end_matches('/').to_string();
        let url = format!("{}/rest/headers/{}/{}.bin", base_url, count, start_hash);
        let request_url = url.clone();

        let res =
            task::spawn_blocking(move || minreq::get(request_url).with_timeout(8).send()).await??;

        if res.status_code != 200 {
            return Err(FetchError::BitcoinCoreREST(format!(
                "could not load headers from REST URL ({}): {} {}: {:?}",
                url,
                res.status_code,
                res.reason_phrase,
                res.as_str(),
            )));
        }

        let header_results: Result<
            Vec<Header>,
            bitcoincore_rpc::bitcoin::consensus::encode::Error,
        > = res
            .as_bytes()
            .chunks(80)
            .map(bitcoin::consensus::deserialize::<Header>)
            .collect();

        let headers = header_results.map_err(|e| {
            FetchError::BitcoinCoreREST(format!(
                "could not deserialize REST header response: {}",
                e
            ))
        })?;

        debug!(
            "loaded {} active-chain headers starting from {} ({})",
            headers.len(),
            start_height,
            start_hash
        );

        Ok(headers)
    }
}

#[async_trait]
impl Node for BitcoinCoreNode {
    fn info(&self) -> &NodeInfo {
        &self.info
    }

    fn endpoint(&self) -> &str {
        &self.rpc_endpoint
    }

    async fn version(&self) -> Result<String, FetchError> {
        self.with_rpc(|rpc| rpc.get_network_info().map(|info| info.subversion))
            .await
    }

    async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError> {
        match locator {
            HeaderLocator::Hash(hash) => {
                self.with_rpc(move |rpc| rpc.get_block_header(&hash)).await
            }
            HeaderLocator::Height(height) => {
                let hash = self.with_rpc(move |rpc| rpc.get_block_hash(height)).await?;
                self.with_rpc(move |rpc| rpc.get_block_header(&hash)).await
            }
        }
    }

    async fn tips(&self) -> Result<Vec<ChainTip>, FetchError> {
        self.with_rpc(|rpc| {
            rpc.get_chain_tips()
                .map(|tips| tips.into_iter().map(Into::into).collect())
        })
        .await
    }

    async fn get_miner_pool(
        &self,
        hash: &BlockHash,
        _height: u64,
        network: bitcoin::Network,
    ) -> Result<Option<String>, FetchError> {
        let hash = *hash;
        let coinbase = self
            .with_rpc(move |rpc| rpc.get_block(&hash))
            .await?
            .txdata
            .into_iter()
            .next()
            .ok_or_else(|| FetchError::DataError(format!("Block {} has no transactions", hash)))?;

        let miner_identification_data = default_data(network);
        Ok(coinbase
            .identify_pool(network, &miner_identification_data)
            .map(|result| result.pool.name))
    }

    async fn get_new_headers(
        &self,
        tips: &[ChainTip],
        tree: &Tree,
        first_tracked_height: u64,
        progress_tx: Option<&tokio::sync::mpsc::UnboundedSender<Vec<HeaderInfo>>>,
    ) -> Result<(Vec<HeaderInfo>, Vec<BlockHash>), FetchError> {
        let mut active_new_headers = if self.use_rest {
            shared_fetch::get_new_active_headers_as_batch(
                self,
                tips,
                tree,
                first_tracked_height,
                progress_tx,
            )
            .await?
        } else {
            shared_fetch::get_new_active_headers_by_height(
                self,
                tips,
                tree,
                first_tracked_height,
                progress_tx,
            )
            .await?
        };

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
            return Err(self.not_supported("mine_new_blocks"));
        }

        self.ensure_wallet_loaded(MINER_WALLET).await?;
        let mining_address = self
            .with_wallet_rpc(MINER_WALLET, |rpc| rpc.get_new_address(None, None))
            .await?
            .assume_checked();
        self.with_rpc(move |rpc| rpc.generate_to_address(count, &mining_address))
            .await
    }

    async fn p2p_network_active(&self) -> Result<bool, FetchError> {
        self.with_rpc(|rpc| rpc.get_network_info().map(|info| info.network_active))
            .await
    }

    async fn set_p2p_network_active(&self, active: bool) -> Result<(), FetchError> {
        self.with_rpc(move |rpc| rpc.set_network_active(active))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_node(network_type: bitcoin::Network) -> BitcoinCoreNode {
        BitcoinCoreNode::new(
            NodeInfo {
                id: 1,
                name: "test".to_string(),
                description: "test node".to_string(),
                implementation: "Bitcoin Core".to_string(),
                network_type,
            },
            "127.0.0.1:18443".to_string(),
            Auth::UserPass("user".to_string(), "pass".to_string()),
            true,
        )
    }

    #[tokio::test]
    async fn mine_new_blocks_rejects_zero_count() {
        let node = test_node(bitcoin::Network::Regtest);
        let result = node.mine_new_blocks(0).await;
        assert!(matches!(result, Err(FetchError::DataError(_))));
    }
}
