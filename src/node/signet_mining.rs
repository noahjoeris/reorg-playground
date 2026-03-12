use std::env;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use bitcoincore_rpc::RpcApi;
use bitcoincore_rpc::bitcoin::BlockHash;
use bitcoincore_rpc::json::{GetBlockTemplateModes, GetBlockTemplateRules};
use hex::encode as hex_encode;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

use crate::error::FetchError;
use crate::node::bitcoin_core::{BitcoinCoreNode, MINER_WALLET};

const DEFAULT_MINER_SCRIPT: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/bitcoin/contrib/signet/miner");
const DEFAULT_TEST_FRAMEWORK: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/bitcoin/test/functional/test_framework"
);
const DEFAULT_PYTHON_BIN: &str = "python3";
const DEFAULT_BITCOIN_CLI_BIN: &str = "bitcoin-cli";
const DEFAULT_BITCOIN_UTIL_BIN: &str = "bitcoin-util";
const RPC_TIMEOUT_SECONDS: u64 = 10;

static SIGNET_MINING_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

struct SignetParams {
    challenge: String,
    nbits: String,
}

struct SignetRuntime {
    miner_script: PathBuf,
    python_bin: String,
    bitcoin_cli_bin: String,
    bitcoin_util_bin: String,
}

pub(super) async fn mine_blocks(
    node: &BitcoinCoreNode,
    count: u64,
) -> Result<Vec<BlockHash>, FetchError> {
    let info = node.node_info();
    let signet = SignetParams {
        challenge: info
            .signet_challenge
            .as_deref()
            .ok_or_else(|| {
                FetchError::DataError("signet_challenge not configured for this network".into())
            })?
            .to_string(),
        nbits: info
            .signet_nbits
            .as_deref()
            .ok_or_else(|| {
                FetchError::DataError("signet_nbits not configured for this network".into())
            })?
            .to_string(),
    };
    let runtime = SignetRuntime::from_env()?;

    let _guard = SIGNET_MINING_LOCK.lock().await;
    node.ensure_wallet_loaded(MINER_WALLET).await?;

    let mut mined_blocks = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let reward_address = next_reward_address(node).await?;
        mined_blocks.push(mine_one_block(node, &signet, &runtime, &reward_address).await?);
    }

    Ok(mined_blocks)
}

impl SignetRuntime {
    fn from_env() -> Result<Self, FetchError> {
        let miner_script = env_path("BITCOIN_CORE_SIGNET_MINER", DEFAULT_MINER_SCRIPT);
        ensure_file_exists(&miner_script, "Bitcoin Core signet miner script")?;

        let test_framework_dir = env_path("BITCOIN_CORE_TEST_FRAMEWORK", DEFAULT_TEST_FRAMEWORK);
        ensure_directory_exists(&test_framework_dir, "Bitcoin Core test framework")?;

        Ok(Self {
            miner_script,
            python_bin: DEFAULT_PYTHON_BIN.to_string(),
            bitcoin_cli_bin: DEFAULT_BITCOIN_CLI_BIN.to_string(),
            bitcoin_util_bin: DEFAULT_BITCOIN_UTIL_BIN.to_string(),
        })
    }

    fn cli_command(
        &self,
        node: &BitcoinCoreNode,
        signet: &SignetParams,
    ) -> Result<String, FetchError> {
        let (rpc_user, rpc_password) = node.rpc_auth().clone().get_user_pass()?;
        let (rpc_host, rpc_port) = rpc_host_and_port(node.rpc_endpoint())?;

        let mut args = vec![
            self.bitcoin_cli_bin.clone(),
            format!("-signetchallenge={}", signet.challenge),
            format!("-rpcclienttimeout={}", RPC_TIMEOUT_SECONDS),
            format!("-rpcconnect={}", rpc_host),
            format!("-rpcport={}", rpc_port),
            format!("-rpcwallet={}", MINER_WALLET),
        ];

        if let Some(rpc_user) = rpc_user {
            args.push(format!("-rpcuser={}", rpc_user));
        }

        if let Some(rpc_password) = rpc_password {
            args.push(format!("-rpcpassword={}", rpc_password));
        }

        Ok(shell_join(&args))
    }

    fn grind_command(&self) -> String {
        shell_join(&[self.bitcoin_util_bin.as_str(), "grind"])
    }
}

async fn next_reward_address(node: &BitcoinCoreNode) -> Result<String, FetchError> {
    Ok(node
        .with_wallet_rpc(MINER_WALLET, |rpc| rpc.get_new_address(None, None))
        .await?
        .assume_checked()
        .to_string())
}

async fn mine_one_block(
    node: &BitcoinCoreNode,
    signet: &SignetParams,
    runtime: &SignetRuntime,
    reward_address: &str,
) -> Result<BlockHash, FetchError> {
    let best_hash_before = node.with_rpc(|rpc| rpc.get_best_block_hash()).await?;
    let block_time = next_block_time(node, signet).await?;

    let output = Command::new(&runtime.python_bin)
        .arg(&runtime.miner_script)
        .arg(format!("--cli={}", runtime.cli_command(node, signet)?))
        .arg("--quiet")
        .arg("generate")
        .arg(format!("--set-block-time={}", block_time))
        .arg(format!("--grind-cmd={}", runtime.grind_command()))
        .arg(format!("--address={}", reward_address))
        .arg(format!("--nbits={}", signet.nbits))
        .output()
        .await
        .map_err(|error| {
            command_error(format!(
                "Could not execute Bitcoin Core signet miner via {}: {}",
                runtime.python_bin, error
            ))
        })?;

    let output_text = combined_output_text(&output.stdout, &output.stderr);
    if !output.status.success() {
        let suffix = if output_text.is_empty() {
            String::new()
        } else {
            format!(": {}", output_text)
        };
        return Err(command_error(format!(
            "Bitcoin Core signet miner failed with status {}{}",
            output.status, suffix
        )));
    }

    if !output_text.is_empty() {
        return Err(command_error(format!(
            "Bitcoin Core signet miner returned unexpected output: {}",
            output_text
        )));
    }

    let best_hash_after = node.with_rpc(|rpc| rpc.get_best_block_hash()).await?;
    if best_hash_after == best_hash_before {
        return Err(command_error(format!(
            "Mining did not advance {} to a new best block",
            node.node_name()
        )));
    }

    Ok(best_hash_after)
}

/// Determines the timestamp to use for the next signet block.
///
/// Fetches a block template from the node to obtain `min_time`, validates that
/// the node's signet challenge matches the expected one, then selects an
/// appropriate block time via [`select_block_time`]. If the chosen time is in
/// the future, this function sleeps until that moment before returning.
async fn next_block_time(node: &BitcoinCoreNode, signet: &SignetParams) -> Result<u64, FetchError> {
    let template = node
        .with_rpc(|rpc| {
            rpc.get_block_template(
                GetBlockTemplateModes::Template,
                &[GetBlockTemplateRules::Signet, GetBlockTemplateRules::SegWit],
                &[],
            )
        })
        .await?;

    let actual_challenge = hex_encode(template.signet_challenge.as_bytes());
    if actual_challenge != signet.challenge {
        return Err(FetchError::DataError(format!(
            "{} returned unexpected signet_challenge: {}",
            node.node_name(),
            actual_challenge
        )));
    }

    let now = unix_timestamp_now()?;
    let block_time = select_block_time(template.min_time, now);
    if block_time > now {
        sleep(Duration::from_secs(block_time - now)).await;
    }

    Ok(block_time)
}

fn unix_timestamp_now() -> Result<u64, FetchError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| command_error(format!("System clock error: {}", error)))?
        .as_secs())
}

fn select_block_time(min_time: u64, now: u64) -> u64 {
    if min_time > now { min_time } else { now }
}

fn env_path(name: &str, default: &str) -> PathBuf {
    env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn ensure_file_exists(path: &Path, label: &str) -> Result<(), FetchError> {
    if path.is_file() {
        return Ok(());
    }

    Err(command_error(format!(
        "Missing {} at {}",
        label,
        path.display()
    )))
}

fn ensure_directory_exists(path: &Path, label: &str) -> Result<(), FetchError> {
    if path.is_dir() {
        return Ok(());
    }

    Err(command_error(format!(
        "Missing {} at {}",
        label,
        path.display()
    )))
}

fn combined_output_text(stdout: &[u8], stderr: &[u8]) -> String {
    String::from_utf8_lossy(&[stdout, stderr].concat())
        .trim()
        .to_string()
}

fn command_error(message: impl Into<String>) -> FetchError {
    FetchError::Command(message.into())
}

fn rpc_host_and_port(endpoint: &str) -> Result<(String, u16), FetchError> {
    let without_scheme = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);
    let without_path = without_scheme.split('/').next().unwrap_or(without_scheme);
    let (host, port) = without_path
        .rsplit_once(':')
        .ok_or_else(|| command_error(format!("Could not parse RPC endpoint '{}'", endpoint)))?;
    let port = port.parse::<u16>().map_err(|error| {
        command_error(format!(
            "Could not parse RPC port from endpoint '{}': {}",
            endpoint, error
        ))
    })?;
    Ok((host.to_string(), port))
}

fn shell_join<S: AsRef<str>>(args: &[S]) -> String {
    args.iter()
        .map(|arg| shell_escape(arg.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_escape(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }

    if arg
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | ':' | '.' | '_' | '-' | '='))
    {
        return arg.to_string();
    }

    format!("'{}'", arg.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_block_time_uses_current_time_when_chain_time_is_stale() {
        assert_eq!(select_block_time(1_598_918_402, 1_778_000_000), 1_778_000_000);
    }

    #[test]
    fn select_block_time_waits_for_future_min_time() {
        assert_eq!(select_block_time(1_778_000_100, 1_778_000_000), 1_778_000_100);
    }

    #[test]
    fn shell_escape_quotes_special_characters() {
        assert_eq!(shell_escape("simple-value"), "simple-value");
        assert_eq!(shell_escape("contains space"), "'contains space'");
        assert_eq!(shell_escape("contains'quote"), "'contains'\"'\"'quote'");
    }

    #[test]
    fn rpc_host_and_port_supports_http_urls() {
        assert_eq!(
            rpc_host_and_port("http://127.0.0.1:38332").unwrap(),
            ("127.0.0.1".to_string(), 38332)
        );
        assert_eq!(
            rpc_host_and_port("bitcoind-signet-a:38332").unwrap(),
            ("bitcoind-signet-a".to_string(), 38332)
        );
    }
}
