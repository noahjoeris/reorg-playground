//! Shared header-fetch orchestration used by all node backend implementations.

use crate::error::{FetchError, JsonRPCError};
use crate::node::{ActiveHeadersBatchProvider, HeaderLocator, Node};
use crate::types::{ChainTip, ChainTipStatus, HeaderInfo, Tree};
use base64::prelude::*;
use bitcoincore_rpc::bitcoin::BlockHash;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use log::{debug, warn};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::max;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc::UnboundedSender;

/// How many active-chain heights to fetch per batch request.
const ACTIVE_BATCH_STEP: i64 = 2000;
/// Maximum active-header count that still triggers miner lookup. Used to limit it in case of large updates.
const ACTIVE_MINER_LOOKUP_LIMIT: usize = 20;
/// How many headers to accumulate before sending one progress batch. Used to update the state already before function returns.
const RPC_PROGRESS_BATCH_SIZE: usize = 10;

/// Derives miner-identification candidates from active and non-active additions.
pub(crate) fn miner_hashes_for_new_headers(
    active_new_headers: &[HeaderInfo],
    nonactive_new_headers: &[HeaderInfo],
) -> Vec<BlockHash> {
    let mut hashes: Vec<BlockHash> = Vec::new();

    if active_new_headers.len() <= ACTIVE_MINER_LOOKUP_LIMIT {
        hashes.extend(active_new_headers.iter().map(|h| h.header.block_hash()));
    }
    hashes.extend(nonactive_new_headers.iter().map(|h| h.header.block_hash()));
    hashes
}

/// Fetches active-chain headers backward from tip using batched transport.
pub(crate) async fn get_new_active_headers_as_batch(
    batch_provider: &dyn ActiveHeadersBatchProvider,
    tips: &[ChainTip],
    tree: &Tree,
    first_tracked_height: u64,
    progress_tx: Option<&UnboundedSender<Vec<HeaderInfo>>>,
) -> Result<Vec<HeaderInfo>, FetchError> {
    let mut new_headers: Vec<HeaderInfo> = Vec::new();
    let active_tip = find_active_tip(tips)?;

    let mut query_height: i64 = active_tip.height as i64;
    loop {
        if query_height < first_tracked_height as i64 {
            break;
        }

        let start_height = max(
            first_tracked_height as i64,
            query_height - ACTIVE_BATCH_STEP + 1,
        );
        let headers = batch_provider
            .batch_active_headers(start_height as u64, ACTIVE_BATCH_STEP as u64)
            .await?;

        if headers.is_empty() {
            break;
        }

        let mut already_knew_a_header = false;
        let mut batch_new: Vec<HeaderInfo> = Vec::new();

        {
            let tree_locked = tree.lock().await;
            for (header, height) in headers.iter().zip(start_height as u64..) {
                if tree_locked.index.contains_key(&header.block_hash()) {
                    already_knew_a_header = true;
                    continue;
                }

                let header_info = make_header_info(height, *header);
                batch_new.push(header_info.clone());
                new_headers.push(header_info);
            }
        }

        send_progress_batch(progress_tx, batch_new);

        if already_knew_a_header {
            break;
        }

        query_height -= ACTIVE_BATCH_STEP;
    }

    new_headers.sort_by_key(|h| h.height);
    Ok(new_headers)
}

/// Fetches active-chain headers backward from tip using one height lookup at a time.
pub(crate) async fn get_new_active_headers_by_height<N: Node + ?Sized>(
    node: &N,
    tips: &[ChainTip],
    tree: &Tree,
    first_tracked_height: u64,
    progress_tx: Option<&UnboundedSender<Vec<HeaderInfo>>>,
) -> Result<Vec<HeaderInfo>, FetchError> {
    let mut new_headers: Vec<HeaderInfo> = Vec::new();
    let mut rpc_buffer: Vec<HeaderInfo> = Vec::new();
    let active_tip = find_active_tip(tips)?;

    let mut query_height: i64 = active_tip.height as i64;
    loop {
        if query_height < first_tracked_height as i64 {
            break;
        }

        let height = query_height as u64;
        let header = node.block_header(HeaderLocator::Height(height)).await?;
        let header_hash = header.block_hash();
        if tree_contains_hash(tree, &header_hash).await {
            break;
        }

        let header_info = make_header_info(height, header);
        rpc_buffer.push(header_info.clone());
        new_headers.push(header_info);

        if rpc_buffer.len() >= RPC_PROGRESS_BATCH_SIZE
            && let Some(tx) = progress_tx
        {
            let _ = tx.send(std::mem::take(&mut rpc_buffer));
        }

        query_height -= 1;
    }

    send_progress_batch(progress_tx, rpc_buffer);
    new_headers.sort_by_key(|h| h.height);
    Ok(new_headers)
}

/// Fetches non-active branch headers for eligible tips using hash-based lookup.
pub(crate) async fn get_new_nonactive_headers_by_hash<N: Node + ?Sized>(
    node: &N,
    tips: &[ChainTip],
    tree: &Tree,
    first_tracked_height: u64,
    progress_tx: Option<&UnboundedSender<Vec<HeaderInfo>>>,
) -> Result<Vec<HeaderInfo>, FetchError> {
    let mut new_headers: Vec<HeaderInfo> = Vec::new();

    for inactive_tip in tips
        .iter()
        .filter(|tip| tip.height.saturating_sub(tip.branchlen as u64) > first_tracked_height)
        .filter(|tip| tip.status != ChainTipStatus::Active)
    {
        let tip_hash = inactive_tip.block_hash().map_err(|e| {
            FetchError::DataError(format!("Invalid block hash '{}': {}", inactive_tip.hash, e))
        })?;

        let mut next_header = tip_hash;
        let mut branch_headers: Vec<HeaderInfo> = Vec::new();

        for i in 0..=inactive_tip.branchlen {
            if tree_contains_hash(tree, &next_header).await {
                break;
            }

            let height = inactive_tip.height.saturating_sub(i as u64);
            debug!(
                "loading non-active-chain header: hash={}, height={}",
                next_header, height
            );

            let header = node.block_header(HeaderLocator::Hash(next_header)).await?;
            let header_info = make_header_info(height, header);

            next_header = header.prev_blockhash;
            branch_headers.push(header_info.clone());
            new_headers.push(header_info);
        }

        send_progress_batch(progress_tx, branch_headers);
    }

    Ok(new_headers)
}

/// Returns the newest active tip or an error if the backend returned none.
fn find_active_tip(tips: &[ChainTip]) -> Result<&ChainTip, FetchError> {
    tips.iter()
        .rfind(|tip| tip.status == ChainTipStatus::Active)
        .ok_or_else(|| FetchError::DataError("No 'active' chain tip returned".to_string()))
}

fn make_header_info(height: u64, header: Header) -> HeaderInfo {
    HeaderInfo {
        header,
        height,
        miner: String::new(),
    }
}

/// Sends progress only when a non-empty batch is available.
fn send_progress_batch(
    progress_tx: Option<&UnboundedSender<Vec<HeaderInfo>>>,
    batch: Vec<HeaderInfo>,
) {
    if !batch.is_empty()
        && let Some(tx) = progress_tx
    {
        let _ = tx.send(batch);
    }
}

async fn tree_contains_hash(tree: &Tree, hash: &BlockHash) -> bool {
    let tree_locked = tree.lock().await;
    tree_locked.index.contains_key(hash)
}

// -- JSON-RPC transport shared by RPC-backed node implementations --

const JSON_RPC_VERSION: &str = "1.0";
static NEXT_JSON_RPC_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize, Debug)]
struct Request {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<Value>,
}

#[derive(Deserialize, Clone)]
struct Error {
    code: i32,
    message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Error(code={}, message='{}')", self.code, self.message)
    }
}

#[derive(Deserialize)]
struct Response<T> {
    jsonrpc: String,
    result: Option<T>,
    error: Option<Error>,
    id: u64,
}

impl<T> Response<T> {
    fn check(&self, req_method: &str, expected_id: u64) -> Option<JsonRPCError> {
        if self.id != expected_id {
            warn!(
                "JSON-RPC response id is {} but expected {}",
                self.id, expected_id
            );
        }
        if self.jsonrpc != JSON_RPC_VERSION {
            warn!(
                "JSON-RPC response version is {} but expected {}",
                self.jsonrpc, JSON_RPC_VERSION
            );
        }
        if let Some(error) = self.error.clone() {
            return Some(JsonRPCError::JsonRpc(format!(
                "JSON RPC response for request '{}' contains error: {}",
                req_method, error
            )));
        }
        None
    }
}

#[derive(Clone)]
pub(crate) struct RpcAuth {
    pub url: String,
    pub user: String,
    pub password: String,
}

pub(crate) fn jsonrpc_call<T: DeserializeOwned>(
    method: &str,
    params: Vec<Value>,
    auth: &RpcAuth,
) -> Result<Option<T>, JsonRPCError> {
    let (id, res) = jsonrpc_request(method, params, auth)?;
    let response: Response<T> = res.json()?;
    if let Some(e) = response.check(method, id) {
        return Err(e);
    }
    Ok(response.result)
}

fn jsonrpc_request(
    method: &str,
    params: Vec<Value>,
    auth: &RpcAuth,
) -> Result<(u64, minreq::Response), JsonRPCError> {
    let id = NEXT_JSON_RPC_ID.fetch_add(1, Ordering::Relaxed);
    let request = Request {
        jsonrpc: String::from(JSON_RPC_VERSION),
        id,
        method: method.to_string(),
        params,
    };

    let token = format!("{}:{}", auth.user, auth.password);

    debug!(
        "JSON-RPC request with user='{}': {:?}",
        auth.user, request
    );

    let res = minreq::post(&auth.url)
        .with_header(
            "Authorization",
            format!("Basic {}", BASE64_STANDARD.encode(&token)),
        )
        .with_header("content-type", "application/json")
        .with_json(&request)?
        .with_timeout(8)
        .send()?;

    debug!("JSON-RPC response for {}: {:?}", method, res.as_str());

    if res.status_code != 200 {
        return Err(JsonRPCError::Http(format!(
            "HTTP request failed: {} {}: {}",
            res.status_code,
            res.reason_phrase,
            res.as_str()?
        )));
    }

    Ok((id, res))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{HeaderLocator, Node, NodeInfo};
    use crate::types::TreeInfo;
    use async_trait::async_trait;
    use bitcoincore_rpc::bitcoin::blockdata::block::Header;
    use bitcoincore_rpc::bitcoin::hashes::{Hash, HashEngine};
    use bitcoincore_rpc::bitcoin::{
        BlockHash, CompactTarget, Network as BitcoinNetwork, TxMerkleNode,
    };
    use petgraph::graph::DiGraph;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tokio::sync::mpsc::unbounded_channel;

    fn make_header(prev: BlockHash, height: u64, nonce_offset: u32) -> Header {
        Header {
            version: bitcoincore_rpc::bitcoin::block::Version::from_consensus(1),
            prev_blockhash: prev,
            merkle_root: TxMerkleNode::all_zeros(),
            time: height as u32,
            bits: CompactTarget::from_consensus(0x1d00ffff),
            nonce: (height as u32).saturating_add(nonce_offset),
        }
    }

    fn make_linear_headers(start_height: u64, end_height: u64) -> Vec<(u64, Header)> {
        let mut headers = Vec::new();
        let mut prev_hash = BlockHash::all_zeros();

        for height in start_height..=end_height {
            let header = make_header(prev_hash, height, 0);
            prev_hash = header.block_hash();
            headers.push((height, header));
        }

        headers
    }

    fn make_tree(headers: &[(u64, Header)]) -> Tree {
        let mut graph: DiGraph<HeaderInfo, bool> = DiGraph::new();
        let mut index = HashMap::new();

        for (height, header) in headers {
            let node_idx = graph.add_node(HeaderInfo {
                header: *header,
                height: *height,
                miner: String::new(),
            });
            index.insert(header.block_hash(), node_idx);
        }

        for idx in graph.node_indices().collect::<Vec<_>>() {
            let prev_hash = graph[idx].header.prev_blockhash;
            if let Some(&parent_idx) = index.get(&prev_hash) {
                graph.update_edge(parent_idx, idx, false);
            }
        }

        Arc::new(Mutex::new(TreeInfo { graph, index }))
    }

    fn make_tip(
        height: u64,
        hash: BlockHash,
        branchlen: usize,
        status: ChainTipStatus,
    ) -> ChainTip {
        ChainTip {
            height,
            hash: hash.to_string(),
            branchlen,
            status,
        }
    }

    fn heights(headers: &[HeaderInfo]) -> Vec<u64> {
        headers.iter().map(|h| h.height).collect()
    }

    /// Drains all currently queued progress batches without waiting for channel close.
    fn drain_progress(
        mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<HeaderInfo>>,
    ) -> Vec<Vec<HeaderInfo>> {
        let mut batches = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            batches.push(batch);
        }
        batches
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum HeaderLookupMode {
        HeightAndHash,
        HeightOnly,
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum ActiveFetchMode {
        Batch,
        Height,
    }

    /// Minimal in-memory `Node` used to exercise shared fetch behavior deterministically.
    #[derive(Clone)]
    struct MockNode {
        info: NodeInfo,
        endpoint: String,
        active_fetch_mode: ActiveFetchMode,
        header_lookup_mode: HeaderLookupMode,
        tips: Vec<ChainTip>,
        headers_by_height: HashMap<u64, Header>,
        headers_by_hash: HashMap<BlockHash, Header>,
    }

    impl MockNode {
        fn new(
            active_fetch_mode: ActiveFetchMode,
            header_lookup_mode: HeaderLookupMode,
            tips: Vec<ChainTip>,
            headers: Vec<(u64, Header)>,
        ) -> Self {
            let mut headers_by_height = HashMap::new();
            let mut headers_by_hash = HashMap::new();

            for (height, header) in headers {
                headers_by_height.insert(height, header);
                headers_by_hash.insert(header.block_hash(), header);
            }

            MockNode {
                info: NodeInfo {
                    id: 1,
                    name: "mock".to_string(),
                    description: "mock node".to_string(),
                    implementation: "mock".to_string(),
                    network_type: BitcoinNetwork::Regtest,
                },
                endpoint: "mock://node".to_string(),
                active_fetch_mode,
                header_lookup_mode,
                tips,
                headers_by_height,
                headers_by_hash,
            }
        }
    }

    #[async_trait]
    impl ActiveHeadersBatchProvider for MockNode {
        async fn batch_active_headers(
            &self,
            start_height: u64,
            count: u64,
        ) -> Result<Vec<Header>, FetchError> {
            if self.active_fetch_mode != ActiveFetchMode::Batch {
                return Err(FetchError::NotSupported {
                    node: self.info.implementation.clone(),
                    operation: "batch_active_headers",
                });
            }

            let mut headers = Vec::new();
            for height in start_height..start_height.saturating_add(count) {
                match self.headers_by_height.get(&height).copied() {
                    Some(header) => headers.push(header),
                    None => break,
                }
            }
            Ok(headers)
        }
    }

    #[async_trait]
    impl Node for MockNode {
        fn info(&self) -> &NodeInfo {
            &self.info
        }

        fn endpoint(&self) -> &str {
            &self.endpoint
        }

        async fn version(&self) -> Result<String, FetchError> {
            Ok("mock".to_string())
        }

        async fn block_header(&self, locator: HeaderLocator) -> Result<Header, FetchError> {
            match locator {
                HeaderLocator::Height(height) => {
                    self.headers_by_height.get(&height).copied().ok_or_else(|| {
                        FetchError::DataError(format!("missing header at height {}", height))
                    })
                }
                HeaderLocator::Hash(hash) => {
                    if self.header_lookup_mode == HeaderLookupMode::HeightOnly {
                        return Err(FetchError::NotSupported {
                            node: self.info.implementation.clone(),
                            operation: "block_header(hash)",
                        });
                    }
                    self.headers_by_hash.get(&hash).copied().ok_or_else(|| {
                        FetchError::DataError(format!("missing header with hash {}", hash))
                    })
                }
            }
        }

        async fn tips(&self) -> Result<Vec<ChainTip>, FetchError> {
            Ok(self.tips.clone())
        }

        async fn get_miner_pool(
            &self,
            _hash: &BlockHash,
            _height: u64,
            _network: bitcoincore_rpc::bitcoin::Network,
        ) -> Result<Option<String>, FetchError> {
            Err(FetchError::NotSupported {
                node: self.info.implementation.clone(),
                operation: "get_miner_pool",
            })
        }

        async fn get_new_headers(
            &self,
            tips: &[ChainTip],
            tree: &Tree,
            first_tracked_height: u64,
            progress_tx: Option<&UnboundedSender<Vec<HeaderInfo>>>,
        ) -> Result<(Vec<HeaderInfo>, Vec<BlockHash>), FetchError> {
            let mut active = match self.active_fetch_mode {
                ActiveFetchMode::Batch => {
                    get_new_active_headers_as_batch(
                        self,
                        tips,
                        tree,
                        first_tracked_height,
                        progress_tx,
                    )
                    .await?
                }
                ActiveFetchMode::Height => {
                    get_new_active_headers_by_height(
                        self,
                        tips,
                        tree,
                        first_tracked_height,
                        progress_tx,
                    )
                    .await?
                }
            };

            let mut nonactive = match get_new_nonactive_headers_by_hash(
                self,
                tips,
                tree,
                first_tracked_height,
                progress_tx,
            )
            .await
            {
                Ok(headers) => headers,
                Err(FetchError::NotSupported { operation, .. })
                    if operation == "block_header(hash)" =>
                {
                    Vec::new()
                }
                Err(e) => return Err(e),
            };

            let miner_hashes = miner_hashes_for_new_headers(&active, &nonactive);
            active.append(&mut nonactive);
            Ok((active, miner_hashes))
        }
    }

    /// Tests that batch-mode active fetch returns only headers missing from the local tree.
    /// It also verifies that progress is emitted as a single batch for this scenario.
    #[tokio::test]
    async fn new_active_headers_batch_returns_unknown_tail() {
        let all_headers = make_linear_headers(0, 25);
        let known_tree = make_tree(&all_headers[..=20]);

        let active_tip_hash = all_headers[25].1.block_hash();
        let node = MockNode::new(
            ActiveFetchMode::Batch,
            HeaderLookupMode::HeightAndHash,
            vec![make_tip(25, active_tip_hash, 0, ChainTipStatus::Active)],
            all_headers,
        );

        let (tx, rx) = unbounded_channel::<Vec<HeaderInfo>>();
        let headers = get_new_active_headers_as_batch(
            &node,
            &node.tips().await.expect("tips"),
            &known_tree,
            0,
            Some(&tx),
        )
        .await
        .expect("new active headers");
        drop(tx);

        assert_eq!(heights(&headers), vec![21, 22, 23, 24, 25]);

        let batches = drain_progress(rx);
        assert_eq!(batches.len(), 1);
        assert_eq!(heights(&batches[0]), vec![21, 22, 23, 24, 25]);
    }

    /// Tests that non-batch active fetch emits progress in fixed-size chunks while scanning backward.
    #[tokio::test]
    async fn new_active_headers_rpc_emits_progress_in_fixed_chunks() {
        let all_headers = make_linear_headers(0, 25);
        let known_tree = make_tree(&all_headers[..=0]);

        let active_tip_hash = all_headers[25].1.block_hash();
        let node = MockNode::new(
            ActiveFetchMode::Height,
            HeaderLookupMode::HeightAndHash,
            vec![make_tip(25, active_tip_hash, 0, ChainTipStatus::Active)],
            all_headers,
        );

        let (tx, rx) = unbounded_channel::<Vec<HeaderInfo>>();
        let headers = get_new_active_headers_by_height(
            &node,
            &node.tips().await.expect("tips"),
            &known_tree,
            0,
            Some(&tx),
        )
        .await
        .expect("new active headers");
        drop(tx);

        assert_eq!(headers.len(), 25);
        assert_eq!(headers.first().map(|h| h.height), Some(1));
        assert_eq!(headers.last().map(|h| h.height), Some(25));

        let batches = drain_progress(rx);
        let batch_sizes: Vec<usize> = batches.iter().map(|batch| batch.len()).collect();
        assert_eq!(batch_sizes, vec![10, 10, 5]);
    }

    /// Tests that small active updates still request miner lookup for active and non-active additions.
    #[tokio::test]
    async fn new_headers_small_active_delta_collects_active_and_nonactive_miner_hashes() {
        let mut all_headers = make_linear_headers(0, 15);

        let hash_13 = all_headers[13].1.block_hash();
        let alt_14 = make_header(hash_13, 14, 1_000_000);
        let alt_15 = make_header(alt_14.block_hash(), 15, 2_000_000);
        all_headers.push((14, alt_14));
        all_headers.push((15, alt_15));

        let known_tree = make_tree(&make_linear_headers(0, 5));

        let active_tip_hash = make_linear_headers(0, 15)[15].1.block_hash();
        let tips = vec![
            make_tip(15, active_tip_hash, 0, ChainTipStatus::Active),
            make_tip(15, alt_15.block_hash(), 1, ChainTipStatus::ValidFork),
        ];

        let node = MockNode::new(
            ActiveFetchMode::Height,
            HeaderLookupMode::HeightAndHash,
            tips,
            all_headers,
        );

        let (headers, miner_hashes) = node
            .get_new_headers(&node.tips().await.expect("tips"), &known_tree, 0, None)
            .await
            .expect("new headers");

        assert_eq!(headers.len(), 12);
        assert_eq!(miner_hashes.len(), 12);
    }

    /// Tests that large active backfills skip active miner lookup and keep non-active miner lookup.
    #[tokio::test]
    async fn new_headers_large_active_delta_collects_only_nonactive_miner_hashes() {
        let mut all_headers = make_linear_headers(0, 25);

        let hash_23 = all_headers[23].1.block_hash();
        let alt_24 = make_header(hash_23, 24, 1_000_000);
        let alt_25 = make_header(alt_24.block_hash(), 25, 2_000_000);
        all_headers.push((24, alt_24));
        all_headers.push((25, alt_25));

        let known_tree = make_tree(&make_linear_headers(0, 0));

        let active_tip_hash = make_linear_headers(0, 25)[25].1.block_hash();
        let tips = vec![
            make_tip(25, active_tip_hash, 0, ChainTipStatus::Active),
            make_tip(25, alt_25.block_hash(), 1, ChainTipStatus::ValidFork),
        ];

        let node = MockNode::new(
            ActiveFetchMode::Height,
            HeaderLookupMode::HeightAndHash,
            tips,
            all_headers,
        );

        let (_headers, miner_hashes) = node
            .get_new_headers(&node.tips().await.expect("tips"), &known_tree, 0, None)
            .await
            .expect("new headers");

        assert_eq!(miner_hashes.len(), 2);
        assert!(miner_hashes.contains(&alt_24.block_hash()));
        assert!(miner_hashes.contains(&alt_25.block_hash()));
    }

    /// Tests that non-active traversal stops when it reaches an ancestor already present in the tree.
    #[tokio::test]
    async fn new_nonactive_headers_stop_at_known_ancestor() {
        let active_headers = make_linear_headers(0, 10);

        let hash_8 = active_headers[8].1.block_hash();
        let alt_9 = make_header(hash_8, 9, 10_000);
        let alt_10 = make_header(alt_9.block_hash(), 10, 20_000);

        let mut all_headers = active_headers.clone();
        all_headers.push((9, alt_9));
        all_headers.push((10, alt_10));

        let known_tree = make_tree(&active_headers[..=8]);

        let node = MockNode::new(
            ActiveFetchMode::Height,
            HeaderLookupMode::HeightAndHash,
            vec![make_tip(
                10,
                alt_10.block_hash(),
                2,
                ChainTipStatus::ValidFork,
            )],
            all_headers,
        );

        let headers = get_new_nonactive_headers_by_hash(
            &node,
            &node.tips().await.expect("tips"),
            &known_tree,
            0,
            None,
        )
        .await
        .expect("nonactive headers");

        assert_eq!(headers.len(), 2);
        assert_eq!(heights(&headers), vec![10, 9]);
    }

    /// Tests that non-active traversal is skipped when hash-based header lookup is unsupported.
    #[tokio::test]
    async fn new_nonactive_headers_skips_when_hash_lookup_not_supported() {
        let active_headers = make_linear_headers(0, 2);
        let known_tree = make_tree(&active_headers[..=0]);
        let active_tip_hash = active_headers[2].1.block_hash();

        let node = MockNode::new(
            ActiveFetchMode::Height,
            HeaderLookupMode::HeightOnly,
            vec![
                make_tip(2, active_tip_hash, 0, ChainTipStatus::Active),
                make_tip(2, active_tip_hash, 1, ChainTipStatus::ValidFork),
            ],
            active_headers,
        );

        let (headers, _miners) = node
            .get_new_headers(&node.tips().await.expect("tips"), &known_tree, 0, None)
            .await
            .expect("new headers");

        assert_eq!(heights(&headers), vec![1, 2]);
    }

    /// Tests that non-active tip filtering uses `saturating_sub` and avoids underflow-driven fetches.
    #[tokio::test]
    async fn new_nonactive_headers_filter_avoids_underflow_with_saturating_sub() {
        let all_headers = make_linear_headers(0, 1);
        let known_tree = make_tree(&all_headers);

        let bogus_hash = {
            let mut engine = BlockHash::engine();
            engine.input(&[42]);
            BlockHash::from_engine(engine)
        };

        let node = MockNode::new(
            ActiveFetchMode::Height,
            HeaderLookupMode::HeightOnly,
            vec![make_tip(1, bogus_hash, 5, ChainTipStatus::ValidFork)],
            all_headers,
        );

        let headers = get_new_nonactive_headers_by_hash(
            &node,
            &node.tips().await.expect("tips"),
            &known_tree,
            0,
            None,
        )
        .await
        .expect("nonactive headers");

        assert!(headers.is_empty());
    }
}
