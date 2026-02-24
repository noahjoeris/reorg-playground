use crate::error::FetchError;
use crate::node::{Capabilities, HeaderLocator, Node, NodeInfo};
use crate::types::{ChainTip, ChainTipStatus};
use async_trait::async_trait;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use bitcoincore_rpc::bitcoin::{BlockHash, Transaction};
use electrum_client::{
    Client as ElectrumClient, ConfigBuilder as ElectrumClientConfigBuilder, ElectrumApi,
};
use std::sync::{Arc, OnceLock};
use std::thread::sleep;
use std::time::Duration;
use tokio::task;

#[derive(Clone)]
pub struct Electrum {
    info: NodeInfo,
    url: String,
    client: Arc<OnceLock<ElectrumClient>>,
}

impl Electrum {
    pub fn new(info: NodeInfo, url: String) -> Self {
        Electrum {
            info,
            url,
            client: Arc::new(OnceLock::new()),
        }
    }

    fn not_supported(&self, operation: &'static str) -> FetchError {
        FetchError::NotSupported {
            node: self.info.implementation.clone(),
            operation,
        }
    }

    fn init_client<'a>(
        client_cell: &'a OnceLock<ElectrumClient>,
        url: &str,
        node_name: &str,
    ) -> &'a ElectrumClient {
        client_cell.get_or_init(|| {
            const ELECTRUM_RECONNECT_DURATION: Duration = Duration::from_secs(60);
            let config = ElectrumClientConfigBuilder::new()
                .timeout(Some(10))
                .retry(2)
                .validate_domain(false)
                .build();

            loop {
                match ElectrumClient::from_config(url, config.clone()) {
                    Ok(client) => {
                        log::info!("Connected to Electrum server {} ({})", node_name, url);
                        return client;
                    }
                    Err(e) => {
                        log::warn!(
                            "Could not connect to Electrum server {}. Retrying in {:?}. Error: {}",
                            url,
                            ELECTRUM_RECONNECT_DURATION,
                            e
                        );
                        sleep(ELECTRUM_RECONNECT_DURATION);
                    }
                }
            }
        })
    }
}

#[async_trait]
impl Node for Electrum {
    fn info(&self) -> &NodeInfo {
        &self.info
    }

    fn endpoint(&self) -> &str {
        &self.url
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_hash_header_lookup: false,
            supports_height_header_lookup: true,
            supports_batch_active_headers: true,
            supports_nonactive_headers: false,
        }
    }

    async fn version(&self) -> Result<String, FetchError> {
        let client_cell = self.client.clone();
        let url = self.url.clone();
        let node_name = self.info.name.clone();

        task::spawn_blocking(move || {
            let client = Self::init_client(client_cell.as_ref(), &url, &node_name);
            client
                .server_features()
                .map(|response| response.server_version)
                .map_err(FetchError::from)
        })
        .await?
    }

    async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError> {
        let height = match locator {
            HeaderLocator::Height(height) => height,
            HeaderLocator::Hash(_) => return Err(self.not_supported("block_header(hash)")),
        };

        let client_cell = self.client.clone();
        let url = self.url.clone();
        let node_name = self.info.name.clone();

        task::spawn_blocking(move || {
            let client = Self::init_client(client_cell.as_ref(), &url, &node_name);
            client
                .block_header(height as usize)
                .map_err(FetchError::from)
        })
        .await?
    }

    async fn block_hash(&self, height: u64) -> Result<BlockHash, FetchError> {
        let client_cell = self.client.clone();
        let url = self.url.clone();
        let node_name = self.info.name.clone();

        task::spawn_blocking(move || {
            let client = Self::init_client(client_cell.as_ref(), &url, &node_name);
            client
                .block_header(height as usize)
                .map(|header| header.block_hash())
                .map_err(FetchError::from)
        })
        .await?
    }

    async fn tips(&self) -> Result<Vec<ChainTip>, FetchError> {
        let client_cell = self.client.clone();
        let url = self.url.clone();
        let node_name = self.info.name.clone();

        task::spawn_blocking(move || {
            let client = Self::init_client(client_cell.as_ref(), &url, &node_name);

            let mut last_header_notification = None;
            loop {
                match client.block_headers_pop() {
                    Ok(option) => match option {
                        Some(notification) => last_header_notification = Some(notification),
                        None => break,
                    },
                    Err(e) => {
                        log::debug!("could not pop block header notification: {}", e);
                        break;
                    }
                }
            }

            if let Some(notification) = last_header_notification {
                return Ok(vec![ChainTip {
                    height: notification.height as u64,
                    hash: notification.header.block_hash().to_string(),
                    branchlen: 0,
                    status: ChainTipStatus::Active,
                }]);
            }

            match client.block_headers_subscribe() {
                Ok(response) => Ok(vec![ChainTip {
                    height: response.height as u64,
                    hash: response.header.block_hash().to_string(),
                    branchlen: 0,
                    status: ChainTipStatus::Active,
                }]),
                Err(e) => {
                    log::warn!("block headers subscribe error, {:?}", e);
                    Err(FetchError::ElectrumClient(e))
                }
            }
        })
        .await?
    }

    async fn coinbase(&self, hash: &BlockHash, height: u64) -> Result<Transaction, FetchError> {
        let expected_hash = *hash;
        let client_cell = self.client.clone();
        let url = self.url.clone();
        let node_name = self.info.name.clone();

        task::spawn_blocking(move || {
            let client = Self::init_client(client_cell.as_ref(), &url, &node_name);

            let header = client
                .block_header(height as usize)
                .map_err(FetchError::from)?;

            if header.block_hash() != expected_hash {
                return Err(FetchError::DataError(
                    "Could not fetch coinbase from non-active chain. Not supported by Electrum."
                        .to_string(),
                ));
            }

            let txid = client
                .txid_from_pos(height as usize, /* coinbase */ 0)
                .map_err(FetchError::from)?;
            client.transaction_get(&txid).map_err(FetchError::from)
        })
        .await?
    }

    async fn batch_active_headers(
        &self,
        start_height: u64,
        count: u64,
    ) -> Result<Vec<Header>, FetchError> {
        let client_cell = self.client.clone();
        let url = self.url.clone();
        let node_name = self.info.name.clone();

        task::spawn_blocking(move || {
            let client = Self::init_client(client_cell.as_ref(), &url, &node_name);
            client
                .block_headers(start_height as usize, count as usize)
                .map(|response| response.headers)
                .map_err(FetchError::from)
        })
        .await?
    }
}
