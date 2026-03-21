use crate::error::FetchError;
use crate::node::shared_fetch;
use crate::node::signet_mining;
use crate::node::{ActiveHeadersBatchProvider, HeaderLocator, Node, NodeInfo, PeerInfo};
use crate::types::{ChainTip, HeaderInfo, Tree};
use async_trait::async_trait;
use bitcoin_pool_identification::{PoolIdentification, default_data};
use bitcoincore_rpc::bitcoin;
use bitcoincore_rpc::bitcoin::BlockHash;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use bitcoincore_rpc::jsonrpc;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use log::debug;
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use tokio::task;

/// Collects every `host:port` representation that may identify the same remote peer.
///
/// Bitcoin Core can report peers by the original hostname or by a resolved `ip:port`. We keep
/// both forms so peer cleanup can match whichever representation Core returns.
fn collect_peer_address_variants(addresses: &[String]) -> HashSet<String> {
    let mut variants = HashSet::new();
    for address in addresses {
        let trimmed_address = address.trim();
        if trimmed_address.is_empty() {
            continue;
        }
        variants.insert(trimmed_address.to_string());
        if let Ok(resolved_addresses) = trimmed_address.to_socket_addrs() {
            for resolved_address in resolved_addresses {
                variants.insert(resolved_address.to_string());
            }
        }
    }
    variants
}

/// Resolves the IPs behind candidate peer addresses for hostname-to-IP fallback matching.
///
/// This is used only when an exact `host:port` match fails and the target is not loopback, since
/// loopback peers often share the same IP and would otherwise be easy to disconnect incorrectly.
fn collect_peer_address_ips(addresses: &[String]) -> HashSet<IpAddr> {
    let mut ips = HashSet::new();
    for address in addresses {
        let trimmed_address = address.trim();
        if trimmed_address.is_empty() {
            continue;
        }
        if let Ok(socket_address) = trimmed_address.parse::<SocketAddr>() {
            ips.insert(socket_address.ip());
        }
        if let Ok(resolved_addresses) = trimmed_address.to_socket_addrs() {
            for resolved_address in resolved_addresses {
                ips.insert(resolved_address.ip());
            }
        }
    }
    ips
}

/// Attempts to disconnect a Bitcoin Core peer if it is still connected.
///
/// Core prefers `disconnectnode` by peer id, but older call sites may only know the address. RPC
/// error `-29` means "not connected", which we treat as an already-satisfied disconnect.
fn try_disconnect_peer(
    rpc: &Client,
    peer_id: u64,
    addr: &str,
) -> Result<(), bitcoincore_rpc::Error> {
    let disconnect_result = match u32::try_from(peer_id) {
        Ok(nid) => rpc.disconnect_node_by_id(nid),
        Err(_) => rpc.disconnect_node(addr),
    };
    match disconnect_result {
        Ok(()) => Ok(()),
        Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::Error::Rpc(ref e)))
            if e.code == -29 =>
        {
            Ok(())
        }
        Err(e) => Err(e),
    }
}

pub(super) const MINER_WALLET: &str = "miner";

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

    pub(super) async fn with_rpc<T, F>(&self, op: F) -> Result<T, FetchError>
    where
        T: Send + 'static,
        F: FnOnce(Client) -> Result<T, bitcoincore_rpc::Error> + Send + 'static,
    {
        let rpc = self.rpc_client()?;
        let result = task::spawn_blocking(move || op(rpc)).await?;
        result.map_err(FetchError::from)
    }

    pub(super) async fn with_wallet_rpc<T, F>(&self, wallet: &str, op: F) -> Result<T, FetchError>
    where
        T: Send + 'static,
        F: FnOnce(Client) -> Result<T, bitcoincore_rpc::Error> + Send + 'static,
    {
        let rpc = self.rpc_client_with_url(&self.wallet_rpc_url(wallet))?;
        let result = task::spawn_blocking(move || op(rpc)).await?;
        result.map_err(FetchError::from)
    }

    pub(super) fn not_supported(&self, operation: &'static str) -> FetchError {
        FetchError::NotSupported {
            node: self.info.implementation.clone(),
            operation,
        }
    }

    pub(super) async fn ensure_wallet_loaded(&self, wallet: &str) -> Result<(), FetchError> {
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

    pub(super) fn node_name(&self) -> &str {
        &self.info.name
    }

    pub(super) fn rpc_auth(&self) -> &Auth {
        &self.rpc_auth
    }

    pub(super) fn rpc_endpoint(&self) -> &str {
        &self.rpc_endpoint
    }

    pub(super) fn node_info(&self) -> &NodeInfo {
        &self.info
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

    fn supports_stale_tips(&self) -> bool {
        true
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
        match self.info.network_type {
            bitcoin::Network::Regtest => {}
            bitcoin::Network::Signet => return signet_mining::mine_blocks(self, count).await,
            _ => return Err(self.not_supported("mine_new_blocks")),
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

    async fn get_peer_info(&self) -> Result<Vec<PeerInfo>, FetchError> {
        self.with_rpc(|rpc| {
            rpc.get_peer_info().map(|peers| {
                peers
                    .into_iter()
                    .map(|p| PeerInfo {
                        id: p.id,
                        addr: p.addr,
                        addrbind: p.addrbind,
                        subver: p.subver,
                        inbound: p.inbound,
                        connection_type: p
                            .connection_type
                            .map(|ct| format!("{:?}", ct).to_lowercase())
                            .unwrap_or_default(),
                        network: p
                            .network
                            .map(|n| format!("{:?}", n).to_lowercase())
                            .unwrap_or_default(),
                    })
                    .collect()
            })
        })
        .await
    }

    async fn add_peer(&self, addr: &str) -> Result<(), FetchError> {
        let addr = addr.to_string();
        self.with_rpc(move |rpc| {
            match rpc.add_node(&addr) {
                Ok(()) => Ok(()),
                // "Node already added" (-23): treat as success.
                Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::Error::Rpc(
                    ref e,
                ))) if e.code == -23 => Ok(()),
                Err(e) => Err(e),
            }
        })
        .await
    }

    /// Removes a peer connection by clearing matching `addnode` entries and dropping the live socket.
    ///
    /// Persistent `addnode "add"` peers reconnect after `disconnectnode` unless `addnode … remove`
    /// is called with the **same string** used when adding. `getpeerinfo`'s `addr` often differs
    /// (e.g. hostname vs IP), so callers pass catalog / connect strings via `addnode_remove_candidates`.
    ///
    /// Uses `disconnectnode` by internal peer id when provided. Treats RPC -29 as success.
    async fn remove_peer_connection(
        &self,
        addr: &str,
        peer_id: Option<u64>,
        addnode_remove_candidates: &[String],
    ) -> Result<(), FetchError> {
        let addr = addr.to_string();
        let mut remove_strings: Vec<String> = addnode_remove_candidates
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        if !addr.is_empty() && !remove_strings.iter().any(|s| s == &addr) {
            remove_strings.push(addr.clone());
        }
        if remove_strings.is_empty() {
            remove_strings.push(addr.clone());
        }

        self.with_rpc(move |rpc| {
            // Discover the exact `addnode` string Core stored (often differs from `getpeerinfo.addr`).
            if let Ok(added) = rpc.get_added_node_info(None) {
                for entry in added {
                    for a in &entry.addresses {
                        if a.address == addr {
                            let s = entry.added_node.clone();
                            if !remove_strings.contains(&s) {
                                remove_strings.push(s);
                            }
                            break;
                        }
                    }
                }
            }

            for s in &remove_strings {
                let _ = rpc.remove_node(s);
            }

            let try_by_addr = || rpc.disconnect_node(&addr);

            let disconnect_result = match peer_id.and_then(|id| u32::try_from(id).ok()) {
                Some(nid) => match rpc.disconnect_node_by_id(nid) {
                    Ok(()) => Ok(()),
                    Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::Error::Rpc(
                        ref e,
                    ))) if e.code == -29 => Ok(()),
                    Err(_) => try_by_addr(),
                },
                None => try_by_addr(),
            };

            match disconnect_result {
                Ok(()) => Ok(()),
                Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::Error::Rpc(
                    ref e,
                ))) if e.code == -29 => Ok(()),
                Err(e) => Err(e),
            }
        })
        .await
    }

    /// Removes this node's side of a symmetric peer relationship after the other node disconnects.
    ///
    /// The caller passes the counterparty's listen addresses. We remove matching `addnode` entries
    /// first, then best-effort disconnect any live sockets that still target those addresses. When
    /// the counterparty resolves to loopback we skip IP-only matching, because multiple local peers
    /// may legitimately share the same loopback address.
    async fn remove_counterparty_peer_connection(
        &self,
        counterparty_listen_address_candidates: &[String],
    ) -> Result<(), FetchError> {
        let counterparty_listen_addresses: Vec<String> = counterparty_listen_address_candidates
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if counterparty_listen_addresses.is_empty() {
            return Ok(());
        }

        let counterparty_address_variants =
            collect_peer_address_variants(&counterparty_listen_addresses);
        let counterparty_ips = collect_peer_address_ips(&counterparty_listen_addresses);
        let any_loopback = counterparty_ips.iter().any(|ip| ip.is_loopback());

        self.with_rpc(move |rpc| {
            for s in &counterparty_listen_addresses {
                let _ = rpc.remove_node(s);
            }

            if let Ok(added) = rpc.get_added_node_info(None) {
                for entry in added {
                    let matched = entry
                        .addresses
                        .iter()
                        .any(|a| counterparty_address_variants.contains(&a.address));
                    if matched {
                        let _ = rpc.remove_node(&entry.added_node);
                    }
                }
            }

            let peers = rpc.get_peer_info()?;
            for p in peers {
                let mut disconnect = counterparty_address_variants.contains(&p.addr);
                if !disconnect && !any_loopback {
                    if let Ok(sa) = p.addr.parse::<SocketAddr>() {
                        disconnect = counterparty_ips.contains(&sa.ip());
                    }
                }
                if disconnect {
                    let _ = try_disconnect_peer(&rpc, p.id, &p.addr);
                }
            }
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_node(id: u32, network_type: bitcoin::Network) -> BitcoinCoreNode {
        BitcoinCoreNode::new(
            NodeInfo {
                id,
                name: "test".to_string(),
                description: "test node".to_string(),
                implementation: "Bitcoin Core".to_string(),
                network_type,
                supports_mining: true,
                signet_challenge: None,
                signet_nbits: None,
                p2p_address: None,
            },
            "127.0.0.1:18443".to_string(),
            Auth::UserPass("user".to_string(), "pass".to_string()),
            true,
        )
    }

    #[tokio::test]
    async fn mine_new_blocks_rejects_zero_count() {
        let node = test_node(1, bitcoin::Network::Regtest);
        let result = node.mine_new_blocks(0).await;
        assert!(matches!(result, Err(FetchError::DataError(_))));
    }
}
