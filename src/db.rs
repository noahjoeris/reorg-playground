use std::collections::HashMap;

use petgraph::graph::DiGraph;
use petgraph::graph::NodeIndex;

use bitcoincore_rpc::bitcoin;
use bitcoincore_rpc::bitcoin::BlockHash;

use log::{debug, info, warn};

use crate::error::DbError;
use crate::types::{Db, HeaderInfo, TreeInfo};

const SELECT_STMT_HEADER_HEIGHT: &str = "
SELECT
    height, header, miner
FROM
    headers
WHERE
    network = ?1
    AND height >= ?2
ORDER BY
    height
    ASC
";

const CREATE_STMT_TABLE_HEADERS: &str = "
CREATE TABLE IF NOT EXISTS headers (
    height     INT,
    network    INT,
    hash       BLOB,
    header     BLOB,
    miner      TEXT,
    PRIMARY KEY (network, hash, header)
)
";

const UPDATE_STMT_HEADER_MINER: &str = "
UPDATE
    headers
SET
    miner = ?1
WHERE
    hash = ?2;
";

pub async fn setup_db(db: Db) -> Result<(), DbError> {
    db.lock().await.execute(CREATE_STMT_TABLE_HEADERS, [])?;
    Ok(())
}

pub async fn write_to_db(
    new_headers: &[HeaderInfo],
    db: Db,
    network: u32,
) -> Result<(), DbError> {
    let mut db_locked = db.lock().await;
    let tx = db_locked.transaction()?;
    debug!(
        "inserting {} headers from network {} into the database..",
        new_headers.len(),
        network
    );
    for info in new_headers {
        tx.execute(
            "INSERT OR IGNORE INTO headers
                   (height, network, hash, header, miner)
                   values (?1, ?2, ?3, ?4, ?5)",
            &[
                &info.height.to_string(),
                &network.to_string(),
                &info.header.block_hash().to_string(),
                &bitcoin::consensus::encode::serialize_hex(&info.header),
                &info.miner,
            ],
        )?;
    }
    tx.commit()?;
    debug!(
        "done inserting {} headers from network {} into the database",
        new_headers.len(),
        network
    );
    Ok(())
}

pub async fn update_miner(db: Db, hash: &BlockHash, miner: String) -> Result<(), DbError> {
    let mut db_locked = db.lock().await;
    let tx = db_locked.transaction()?;

    tx.execute(UPDATE_STMT_HEADER_MINER, [miner, hash.to_string()])?;
    tx.commit()?;
    Ok(())
}

// Loads header and tip information for a specified network from the DB and
// builds a header-tree from it. Only loads headers at or above first_tracked_height.
pub async fn load_treeinfos(
    db: Db,
    network: u32,
    first_tracked_height: u64,
) -> Result<TreeInfo, DbError> {
    let header_infos = load_header_infos(db, network, first_tracked_height).await?;

    let mut graph: DiGraph<HeaderInfo, bool> = DiGraph::new();
    let mut index: HashMap<BlockHash, NodeIndex> = HashMap::new();
    info!("building header tree for network {}..", network);
    for h in header_infos.iter() {
        let idx = graph.add_node(h.clone());
        index.insert(h.header.block_hash(), idx);
    }
    info!(".. added headers from network {}", network);
    for current in header_infos {
        let idx_current = index
            .get(&current.header.block_hash())
            .expect("header was just inserted");
        match index.get(&current.header.prev_blockhash) {
            Some(idx_prev) => graph.update_edge(*idx_prev, *idx_current, false),
            None => continue,
        };
    }
    info!(
        ".. added relationships between headers from network {}",
        network
    );
    let root_nodes = graph.externals(petgraph::Direction::Incoming).count();
    info!(
        "done building header tree for network {}: roots={}, tips={}",
        network,
        root_nodes,
        graph.externals(petgraph::Direction::Outgoing).count(),
    );
    if root_nodes > 1 {
        warn!(
            "header-tree for network {} has more than one ({}) root!",
            network, root_nodes
        );
    }
    Ok(TreeInfo { graph, index })
}

async fn load_header_infos(
    db: Db,
    network: u32,
    first_tracked_height: u64,
) -> Result<Vec<HeaderInfo>, DbError> {
    info!(
        "loading headers for network {} from database (first_tracked_height={})..",
        network, first_tracked_height
    );
    let db_locked = db.lock().await;

    let mut stmt = db_locked.prepare(SELECT_STMT_HEADER_HEIGHT)?;

    let mut headers: Vec<HeaderInfo> = vec![];

    let mut rows = stmt.query([network.to_string(), first_tracked_height.to_string()])?;
    while let Some(row) = rows.next()? {
        let header_hex: String = row.get(1)?;
        let header_bytes = hex::decode(&header_hex)?;
        let header = bitcoin::consensus::deserialize(&header_bytes)?;
        headers.push(HeaderInfo {
            height: row.get(0)?,
            header,
            miner: row.get(2)?,
        });
    }

    info!(
        "done loading headers for network {}: headers={}",
        network,
        headers.len()
    );

    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoincore_rpc::bitcoin::blockdata::block::Header;
    use bitcoincore_rpc::bitcoin::hashes::Hash;
    use bitcoincore_rpc::bitcoin::{BlockHash, CompactTarget, TxMerkleNode};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn make_header(prev: BlockHash, height: u64) -> Header {
        Header {
            version: bitcoincore_rpc::bitcoin::block::Version::from_consensus(1),
            prev_blockhash: prev,
            merkle_root: TxMerkleNode::all_zeros(),
            time: height as u32,
            bits: CompactTarget::from_consensus(0x1d00ffff),
            nonce: height as u32,
        }
    }

    fn make_linear_headers(start_height: u64, end_height: u64) -> Vec<HeaderInfo> {
        let mut headers: Vec<HeaderInfo> = vec![];
        let mut prev_hash = BlockHash::all_zeros();
        for height in start_height..=end_height {
            let header = make_header(prev_hash, height);
            let hash = header.block_hash();
            headers.push(HeaderInfo {
                height,
                header,
                miner: String::new(),
            });
            prev_hash = hash;
        }
        headers
    }

    #[tokio::test]
    async fn load_treeinfos_respects_first_tracked_height() {
        let connection = rusqlite::Connection::open_in_memory().expect("open in-memory sqlite");
        let db: Db = Arc::new(Mutex::new(connection));
        setup_db(db.clone()).await.expect("setup db");

        let network_id = 42;
        let headers = make_linear_headers(100, 110);
        write_to_db(&headers, db.clone(), network_id)
            .await
            .expect("write headers");

        let tree = load_treeinfos(db, network_id, 105)
            .await
            .expect("load treeinfos");
        let heights: Vec<u64> = tree.graph.raw_nodes().iter().map(|n| n.weight.height).collect();

        assert_eq!(tree.graph.node_count(), 6);
        assert!(heights.iter().all(|h| *h >= 105));
        assert!(heights.contains(&105));
        assert!(!heights.contains(&104));
    }
}
