use crate::error::{EsploraRESTError, FetchError};
use crate::node::shared_fetch;
use crate::node::{HeaderLocator, Node, NodeInfo};
use crate::types::{ChainTip, ChainTipStatus, HeaderInfo, Tree};
use async_trait::async_trait;
use bitcoin_pool_identification::{PoolIdentification, default_data};
use bitcoincore_rpc::bitcoin;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use bitcoincore_rpc::bitcoin::hex::FromHex;
use bitcoincore_rpc::bitcoin::{BlockHash, Transaction};
use std::str::FromStr;
use tokio::task;

#[derive(Hash, Clone)]
pub struct Esplora {
    info: NodeInfo,
    api_url: String,
}

impl Esplora {
    pub fn new(info: NodeInfo, api_url: String) -> Self {
        Esplora { info, api_url }
    }

    fn not_supported(&self, operation: &'static str) -> FetchError {
        FetchError::NotSupported {
            node: self.info.implementation.clone(),
            operation,
        }
    }

    async fn get_text(&self, url: String) -> Result<String, FetchError> {
        let request_url = url.clone();
        let response = task::spawn_blocking(move || {
            minreq::get(request_url)
                .with_header("content-type", "plain/text")
                .with_timeout(8)
                .send()
        })
        .await??;

        if response.status_code != 200 {
            let body = response.as_str().unwrap_or("<unreadable body>");
            return Err(FetchError::EsploraREST(EsploraRESTError::Http(format!(
                "HTTP request to {} failed: {} {}: {}",
                url, response.status_code, response.reason_phrase, body
            ))));
        }

        Ok(response.as_str()?.to_string())
    }

    async fn block_hash_at_height(&self, height: u64) -> Result<BlockHash, FetchError> {
        let url = format!("{}/block-height/{}", self.api_url, height);
        let hash_str = self.get_text(url).await?;
        BlockHash::from_str(hash_str.trim())
            .map_err(|e| FetchError::DataError(format!("Invalid block hash '{}': {}", hash_str, e)))
    }
}

fn decode_header_hex(header_hex: &str) -> Result<Header, FetchError> {
    let header_bytes = Vec::from_hex(header_hex).map_err(|e| {
        FetchError::DataError(format!(
            "Can't hex decode block header '{}': {}",
            header_hex, e
        ))
    })?;

    bitcoin::consensus::deserialize(&header_bytes).map_err(|e| {
        FetchError::DataError(format!(
            "Can't deserialize block header '{}': {}",
            header_hex, e
        ))
    })
}

fn decode_coinbase_from_responses(
    _txid_response: &str,
    tx_hex_response: &str,
) -> Result<Transaction, FetchError> {
    let tx_bytes = Vec::from_hex(tx_hex_response).map_err(|e| {
        FetchError::DataError(format!(
            "Can't hex decode coinbase transaction '{}': {}",
            tx_hex_response, e
        ))
    })?;

    bitcoin::consensus::deserialize(&tx_bytes).map_err(|e| {
        FetchError::DataError(format!(
            "Can't deserialize coinbase transaction '{}': {}",
            tx_hex_response, e
        ))
    })
}

#[async_trait]
impl Node for Esplora {
    fn info(&self) -> &NodeInfo {
        &self.info
    }

    fn endpoint(&self) -> &str {
        &self.api_url
    }

    async fn version(&self) -> Result<String, FetchError> {
        Err(self.not_supported("version"))
    }

    async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError> {
        let hash = match locator {
            HeaderLocator::Hash(hash) => hash,
            HeaderLocator::Height(height) => self.block_hash_at_height(height).await?,
        };

        let url = format!("{}/block/{}/header", self.api_url, hash);
        let header_hex = self.get_text(url).await?;
        decode_header_hex(&header_hex)
    }

    async fn get_miner_pool(
        &self,
        hash: &BlockHash,
        _height: u64,
        network: bitcoin::Network,
    ) -> Result<Option<String>, FetchError> {
        let txid_url = format!("{}/block/{}/txid/0", self.api_url, hash);
        let txid = self.get_text(txid_url).await?;

        let tx_hex_url = format!("{}/tx/{}/hex", self.api_url, txid.trim());
        let tx_hex = self.get_text(tx_hex_url).await?;

        let coinbase = decode_coinbase_from_responses(&txid, &tx_hex)?;
        let miner_identification_data = default_data(network);
        Ok(coinbase
            .identify_pool(network, &miner_identification_data)
            .map(|result| result.pool.name))
    }

    async fn tips(&self) -> Result<Vec<ChainTip>, FetchError> {
        let url = format!("{}/blocks/tip/height", self.api_url);
        let height_str = self.get_text(url).await?;

        let height = height_str.trim().parse::<u64>().map_err(|e| {
            FetchError::DataError(format!("Invalid block height '{}': {}", height_str, e))
        })?;

        let hash = self.block_hash_at_height(height).await?;
        Ok(vec![ChainTip {
            height,
            hash: hash.to_string(),
            branchlen: 0,
            status: ChainTipStatus::Active,
        }])
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

#[cfg(test)]
mod tests {
    use super::decode_coinbase_from_responses;

    #[test]
    fn decode_coinbase_uses_tx_hex_response() {
        let genesis_coinbase_hex = "01000000010000000000000000000000000000000000000000000000000000000000000000000000004d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000";

        let tx = decode_coinbase_from_responses("not-a-tx-hex", genesis_coinbase_hex)
            .expect("coinbase tx should be decoded from the tx hex response body");

        assert_eq!(tx.input.len(), 1);
        assert_eq!(tx.output.len(), 1);
    }
}
