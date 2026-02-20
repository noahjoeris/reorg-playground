use std::fmt;
use std::str::FromStr;

use crate::error::JsonRPCError;
use crate::types::ChainTip;

use bitcoincore_rpc::bitcoin;
use bitcoincore_rpc::bitcoin::blockdata::block::Header;
use bitcoincore_rpc::bitcoin::Block;

use base64::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use log::{debug, warn};

const JSON_RPC_VERSION: &str = "1.0";
const JSON_RPC_ID: u64 = 45324;
const BITCOIN_BLOCK_HEADER_HEX_LENGTH: usize = 80 * 2;
const BITCOIN_BLOCK_HASH_HEX_LENGTH: usize = 32 * 2;

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
    fn check(&self, req_method: &str) -> Option<JsonRPCError> {
        if self.id != JSON_RPC_ID {
            warn!(
                "JSON-RPC response id is {} but expected {}",
                self.id, JSON_RPC_ID
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

pub fn btcd_chaintips(
    url: &str,
    user: &str,
    password: &str,
) -> Result<Vec<ChainTip>, JsonRPCError> {
    const METHOD: &str = "getchaintips";

    let res = request(METHOD, vec![], url, user, password)?;
    let jsonrpc_response: Response<Vec<ChainTip>> = res.json()?;
    if let Some(e) = jsonrpc_response.check(METHOD) {
        return Err(e);
    }

    jsonrpc_response.result.ok_or_else(|| {
        JsonRPCError::JsonRpc(format!(
            "JSON RPC response for request '{}' was empty.",
            METHOD
        ))
    })
}

pub fn btcd_blockheader(
    url: &str,
    user: &str,
    password: &str,
    hash: &str,
) -> Result<Header, JsonRPCError> {
    const METHOD: &str = "getblockheader";
    const PARAM_VERBOSE: bool = false;

    let res = request(
        METHOD,
        vec![Value::from(hash), Value::from(PARAM_VERBOSE)],
        url,
        user,
        password,
    )?;
    let jsonrpc_response: Response<String> = res.json()?;
    if let Some(e) = jsonrpc_response.check(METHOD) {
        return Err(e);
    }

    let header_hex = jsonrpc_response.result.unwrap_or_default();

    if header_hex.len() != BITCOIN_BLOCK_HEADER_HEX_LENGTH {
        return Err(JsonRPCError::RpcUnexpectedResponseContents(format!(
            "JSON RPC response for request '{}' has not the correct length for a Bitcoin block header. Expected {} hex chars but got {} chars. Content: {}",
            METHOD, BITCOIN_BLOCK_HEADER_HEX_LENGTH, header_hex.len(), header_hex
        )));
    }

    let header_bytes = hex::decode(header_hex)?;
    let header: Header = bitcoin::consensus::deserialize(&header_bytes)?;
    Ok(header)
}

pub fn btcd_block(
    url: &str,
    user: &str,
    password: &str,
    hash: &str,
) -> Result<Block, JsonRPCError> {
    const METHOD: &str = "getblock";
    const PARAM_VERBOSE: i8 = 0;

    let res = request(
        METHOD,
        vec![Value::from(hash), Value::from(PARAM_VERBOSE)],
        url,
        user,
        password,
    )?;
    let jsonrpc_response: Response<String> = res.json()?;
    if let Some(e) = jsonrpc_response.check(METHOD) {
        return Err(e);
    }

    let block_hex = jsonrpc_response.result.unwrap_or_default();
    let block_bytes = hex::decode(block_hex)?;
    let block: Block = bitcoin::consensus::deserialize(&block_bytes)?;
    Ok(block)
}

pub fn btcd_blockhash(
    url: &str,
    user: &str,
    password: &str,
    height: u64,
) -> Result<bitcoin::BlockHash, JsonRPCError> {
    const METHOD: &str = "getblockhash";

    let res = request(METHOD, vec![Value::from(height)], url, user, password)?;
    let jsonrpc_response: Response<String> = res.json()?;
    if let Some(e) = jsonrpc_response.check(METHOD) {
        return Err(e);
    }

    let hash_hex = jsonrpc_response.result.unwrap_or_default();

    if hash_hex.len() != BITCOIN_BLOCK_HASH_HEX_LENGTH {
        return Err(JsonRPCError::RpcUnexpectedResponseContents(format!(
            "JSON RPC response for request '{}' has not the correct length for a Bitcoin block hash. Expected {} hex chars but got {} chars. Content: {}",
            METHOD, BITCOIN_BLOCK_HASH_HEX_LENGTH, hash_hex.len(), hash_hex
        )));
    }

    Ok(bitcoin::BlockHash::from_str(&hash_hex)?)
}

fn request(
    method: &str,
    params: Vec<Value>,
    url: &str,
    user: &str,
    password: &str,
) -> Result<minreq::Response, JsonRPCError> {
    let jsonrpc_request = Request {
        jsonrpc: String::from(JSON_RPC_VERSION),
        id: JSON_RPC_ID,
        method: method.to_string(),
        params,
    };

    let token = format!("{}:{}", user, password);

    debug!(
        "JSON-RPC request with user='{}': {:?}",
        user, jsonrpc_request
    );

    let res = minreq::post(url)
        .with_header(
            "Authorization",
            format!("Basic {}", BASE64_STANDARD.encode(&token)),
        )
        .with_header("content-type", "plain/text")
        .with_json(&jsonrpc_request)?
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

    Ok(res)
}
