use crate::error::FetchError;
use crate::node::{Capabilities, HeaderLocator, Node, NodeInfo};
use crate::types::ChainTip;
use async_trait::async_trait;
use bitcoincore_rpc::bitcoin;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use bitcoincore_rpc::bitcoin::{BlockHash, Transaction};
use bitcoincore_rpc::jsonrpc;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use log::debug;
use tokio::task;

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

    fn rpc_client(&self) -> Result<Client, FetchError> {
        let (user, pass) = self.rpc_auth.clone().get_user_pass()?;
        let rpc_url = self.normalized_rpc_url();

        let mut transport_builder = jsonrpc::minreq_http::MinreqHttpTransport::builder()
            .url(&rpc_url)
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

    fn not_supported(&self, operation: &'static str) -> FetchError {
        FetchError::NotSupported {
            node: self.info.implementation.clone(),
            operation,
        }
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

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_hash_header_lookup: true,
            supports_height_header_lookup: false,
            supports_batch_active_headers: self.use_rest,
            supports_nonactive_headers: true,
        }
    }

    async fn version(&self) -> Result<String, FetchError> {
        self.with_rpc(|rpc| rpc.get_network_info().map(|info| info.subversion))
            .await
    }

    async fn block_hash(&self, height: u64) -> Result<BlockHash, FetchError> {
        self.with_rpc(move |rpc| rpc.get_block_hash(height)).await
    }

    async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError> {
        match locator {
            HeaderLocator::Hash(hash) => {
                self.with_rpc(move |rpc| rpc.get_block_header(&hash)).await
            }
            HeaderLocator::Height(_) => Err(self.not_supported("block_header(height)")),
        }
    }

    async fn batch_active_headers(
        &self,
        start_height: u64,
        count: u64,
    ) -> Result<Vec<Header>, FetchError> {
        if !self.use_rest {
            return Err(self.not_supported("batch_active_headers"));
        }

        let start_hash = self.block_hash(start_height).await?;
        debug!(
            "loading active-chain headers starting from {} ({})",
            start_height, start_hash
        );

        let base_url = self.normalized_rpc_url().trim_end_matches('/').to_string();
        let url = format!("{}/rest/headers/{}/{}.bin", base_url, count, start_hash);
        let request_url = url.clone();

        let res = task::spawn_blocking(move || minreq::get(request_url).with_timeout(8).send())
            .await??;

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

    async fn tips(&self) -> Result<Vec<ChainTip>, FetchError> {
        self.with_rpc(|rpc| {
            rpc.get_chain_tips()
                .map(|tips| tips.into_iter().map(Into::into).collect())
        })
        .await
    }

    async fn coinbase(&self, hash: &BlockHash, _height: u64) -> Result<Transaction, FetchError> {
        let hash = *hash;
        self.with_rpc(move |rpc| rpc.get_block(&hash)).await?.txdata.into_iter().next().ok_or_else(
            || FetchError::DataError(format!("Block {} has no transactions", hash)),
        )
    }
}
