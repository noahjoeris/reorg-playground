use crate::error::ConfigError;
use crate::node::{BitcoinCoreNode, BtcdNode, Electrum, Esplora, Node, NodeInfo};
use bitcoincore_rpc::Auth;
use bitcoincore_rpc::bitcoin::Network as BitcoinNetwork;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::{env, fmt, fs};

pub const ENVVAR_CONFIG_FILE: &str = "CONFIG_FILE";
const DEFAULT_CONFIG: &str = "config.toml";
const DEFAULT_USE_REST: bool = true;
const DEFAULT_RPC_PORT: u16 = 8332;

#[derive(Clone, Deserialize, Serialize, Debug)]
pub enum NetworkType {
    Mainnet,
    Testnet,
    Signet,
    Regtest,
}

impl NetworkType {
    pub fn as_bitcoin_network(&self) -> BitcoinNetwork {
        match self {
            NetworkType::Mainnet => BitcoinNetwork::Bitcoin,
            NetworkType::Testnet => BitcoinNetwork::Testnet,
            NetworkType::Signet => BitcoinNetwork::Signet,
            NetworkType::Regtest => BitcoinNetwork::Regtest,
        }
    }
}

#[derive(Deserialize)]
struct TomlConfig {
    address: String,
    database_path: String,
    rss_base_url: Option<String>,
    networks: Vec<TomlNetwork>,
}

#[derive(Clone)]
pub struct Config {
    pub database_path: PathBuf,
    pub address: SocketAddr,
    pub networks: Vec<Network>,
    pub rss_base_url: String,
}

#[derive(Debug, Deserialize)]
struct TomlNetwork {
    id: u32,
    name: String,
    description: String,
    query_interval: u64,
    first_tracked_height: u64,
    visible_heights_from_tip: usize,
    extra_hotspot_heights: usize,
    network_type: NetworkType,
    #[serde(default)]
    disable_node_controls: bool,
    nodes: Vec<TomlNode>,
}

#[derive(Clone)]
pub struct Network {
    pub id: u32,
    pub description: String,
    pub name: String,
    pub query_interval: Duration,
    pub first_tracked_height: u64,
    pub visible_heights_from_tip: usize,
    pub extra_hotspot_heights: usize,
    pub network_type: NetworkType,
    pub disable_node_controls: bool,
    pub nodes: Vec<Arc<dyn Node>>,
}

impl fmt::Display for TomlNetwork {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Network (id={}, description='{}', name='{}', query_interval={}, first_tracked_height={}, visible_heights_from_tip={}, extra_hotspot_heights={}, disable_node_controls={}, nodes={:?})",
            self.id,
            self.description,
            self.name,
            self.query_interval,
            self.first_tracked_height,
            self.visible_heights_from_tip,
            self.extra_hotspot_heights,
            self.disable_node_controls,
            self.nodes,
        )
    }
}

#[derive(Debug, Deserialize)]
struct TomlNode {
    id: u32,
    description: String,
    name: String,
    rpc_host: String,
    rpc_port: Option<u16>,
    rpc_cookie_file: Option<PathBuf>,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
    use_rest: Option<bool>,
    client_implementation: String,
}

impl fmt::Display for TomlNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Node (id={}, description='{}', name='{}', rpc_host='{}', rpc_port={}, rpc_user='{}', rpc_password='***', rpc_cookie_file={:?}, use_rest={}, client_implementation='{}')",
            self.id,
            self.description,
            self.name,
            self.rpc_host,
            self.rpc_port.unwrap_or(DEFAULT_RPC_PORT),
            self.rpc_user.as_ref().unwrap_or(&"".to_string()),
            self.rpc_cookie_file,
            self.use_rest.unwrap_or(DEFAULT_USE_REST),
            self.client_implementation,
        )
    }
}

#[derive(Hash, Clone)]
pub enum Backend {
    BitcoinCore,
    Btcd,
    /// An esplora based backend.
    Esplora,
    /// An Electrum server as backend.
    Electrum,
}

impl FromStr for Backend {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_lowercase().replace([' ', '_', '-'], "");
        match s.as_str() {
            "bitcoincore" => Ok(Backend::BitcoinCore),
            "btcd" => Ok(Backend::Btcd),
            "esplora" => Ok(Backend::Esplora),
            "electrum" => Ok(Backend::Electrum),
            _ => Err(ConfigError::UnknownImplementation),
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Backend::BitcoinCore => write!(f, "Bitcoin Core"),
            Backend::Btcd => write!(f, "btcd"),
            Backend::Esplora => write!(f, "esplora"),
            Backend::Electrum => write!(f, "electrum"),
        }
    }
}

fn parse_rpc_auth(node_config: &TomlNode) -> Result<Auth, ConfigError> {
    if let Some(rpc_cookie_file) = node_config.rpc_cookie_file.clone() {
        if !rpc_cookie_file.exists() {
            return Err(ConfigError::CookieFileDoesNotExist);
        }
        return Ok(Auth::CookieFile(rpc_cookie_file));
    }
    if let (Some(user), Some(password)) = (
        node_config.rpc_user.clone(),
        node_config.rpc_password.clone(),
    ) {
        return Ok(Auth::UserPass(user, password));
    }
    Err(ConfigError::NoBitcoinCoreRpcAuth)
}

pub fn load_config() -> Result<Config, ConfigError> {
    let config_file_path =
        env::var(ENVVAR_CONFIG_FILE).unwrap_or_else(|_| DEFAULT_CONFIG.to_string());
    info!("Reading configuration file from {}.", config_file_path);
    let config_string = fs::read_to_string(config_file_path)?;
    parse_config(&config_string)
}

fn parse_config(config_str: &str) -> Result<Config, ConfigError> {
    let toml_config: TomlConfig = toml::from_str(config_str)?;

    let mut networks: Vec<Network> = vec![];
    let mut network_ids: Vec<u32> = vec![];

    for toml_network in toml_config.networks.iter() {
        let network_type = toml_network.network_type.as_bitcoin_network();
        let mut nodes: Vec<Arc<dyn Node>> = vec![];
        let mut node_ids: Vec<u32> = vec![];

        for toml_node in toml_network.nodes.iter() {
            match parse_toml_node(toml_node, network_type) {
                Ok(node) => {
                    let node_id = node.info().id;
                    if node_ids.contains(&node_id) {
                        error!(
                            "Duplicate node id {}: The node {} could not be loaded.",
                            node_id,
                            node.info()
                        );
                        return Err(ConfigError::DuplicateNodeId);
                    }
                    node_ids.push(node_id);
                    nodes.push(node);
                }
                Err(e) => {
                    error!("Error while parsing a node configuration: {}", toml_node);
                    return Err(e);
                }
            }
        }

        match parse_toml_network(toml_network, nodes) {
            Ok(network) => {
                if !network_ids.contains(&network.id) {
                    network_ids.push(network.id);
                    networks.push(network);
                } else {
                    error!(
                        "Duplicate network id {}: The network {} could not be loaded.",
                        network.id, network.name
                    );
                    return Err(ConfigError::DuplicateNetworkId);
                }
            }
            Err(e) => {
                error!(
                    "Error while parsing a network configuration: {:?}",
                    toml_network,
                );
                return Err(e);
            }
        }
    }

    if networks.is_empty() {
        return Err(ConfigError::NoNetworks);
    }

    Ok(Config {
        database_path: PathBuf::from(toml_config.database_path),
        address: SocketAddr::from_str(&toml_config.address)?,
        rss_base_url: toml_config.rss_base_url.unwrap_or_default().clone(),
        networks,
    })
}

fn parse_toml_network(
    toml_network: &TomlNetwork,
    nodes: Vec<Arc<dyn Node>>,
) -> Result<Network, ConfigError> {
    Ok(Network {
        id: toml_network.id,
        name: toml_network.name.clone(),
        description: toml_network.description.clone(),
        query_interval: Duration::from_secs(toml_network.query_interval),
        first_tracked_height: toml_network.first_tracked_height,
        visible_heights_from_tip: toml_network.visible_heights_from_tip,
        extra_hotspot_heights: toml_network.extra_hotspot_heights,
        network_type: toml_network.network_type.clone(),
        disable_node_controls: toml_network.disable_node_controls,
        nodes,
    })
}

fn parse_toml_node(
    toml_node: &TomlNode,
    network_type: BitcoinNetwork,
) -> Result<Arc<dyn Node>, ConfigError> {
    let client_implementation = toml_node.client_implementation.parse::<Backend>()?;

    let node_info = NodeInfo {
        id: toml_node.id,
        name: toml_node.name.clone(),
        description: toml_node.description.clone(),
        implementation: client_implementation.to_string(),
        network_type,
    };

    match client_implementation {
        Backend::BitcoinCore => Ok(Arc::new(BitcoinCoreNode::new(
            node_info,
            format!(
                "{}:{}",
                toml_node.rpc_host,
                toml_node.rpc_port.unwrap_or(DEFAULT_RPC_PORT)
            ),
            parse_rpc_auth(toml_node)?,
            toml_node.use_rest.unwrap_or(DEFAULT_USE_REST),
        ))),
        Backend::Btcd => {
            if toml_node.rpc_user.is_none() || toml_node.rpc_password.is_none() {
                return Err(ConfigError::NoBtcdRpcAuth);
            }

            let node: Arc<dyn Node> = Arc::new(BtcdNode::new(
                node_info,
                format!(
                    "{}:{}",
                    toml_node.rpc_host,
                    toml_node.rpc_port.unwrap_or(DEFAULT_RPC_PORT)
                ),
                toml_node.rpc_user.clone().expect("a rpc_user for btcd"),
                toml_node
                    .rpc_password
                    .clone()
                    .expect("a rpc_password for btcd"),
            ));
            Ok(node)
        }
        Backend::Esplora => Ok(Arc::new(Esplora::new(
            node_info,
            toml_node.rpc_host.clone(),
        ))),
        Backend::Electrum => {
            let url = format!(
                "{}:{}",
                toml_node.rpc_host,
                toml_node.rpc_port.unwrap_or(50002)
            );
            Ok(Arc::new(Electrum::new(node_info, url)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ConfigError;

    #[test]
    fn error_on_duplicate_node_id_test() {
        if let Err(ConfigError::DuplicateNodeId) = parse_config(
            r#"
            database_path = ""
            www_path = "./www"
            query_interval = 15
            address = "127.0.0.1:2323"
            rss_base_url = ""
            footer_html = ""

            [[networks]]
            id = 1
            name = ""
            description = ""
            query_interval = 15
            first_tracked_height = 0
            visible_heights_from_tip = 0
            extra_hotspot_heights = 0
            network_type = "Regtest"

                [[networks.nodes]]
                id = 0
                name = "Node A"
                description = ""
                rpc_host = "127.0.0.1"
                rpc_port = 0
                rpc_user = ""
                rpc_password = ""
                client_implementation = "bitcoincore"

                [[networks.nodes]]
                id = 0
                name = "Node B"
                description = ""
                rpc_host = "127.0.0.1"
                rpc_port = 0
                rpc_user = ""
                rpc_password = ""
                client_implementation = "bitcoincore"
        "#,
        ) {
            // test OK, as we expect this to error
        } else {
            panic!("Test did not error!");
        }
    }

    #[test]
    fn error_on_duplicate_network_id_test() {
        if let Err(ConfigError::DuplicateNetworkId) = parse_config(
            r#"
            database_path = ""
            www_path = "./www"
            query_interval = 15
            address = "127.0.0.1:2323"
            rss_base_url = ""
            footer_html = ""

            [[networks]]
            id = 1
            name = ""
            description = ""
            query_interval = 15
            first_tracked_height = 0
            visible_heights_from_tip = 0
            extra_hotspot_heights = 0
            network_type = "Regtest"

                [[networks.nodes]]
                id = 0
                name = "Node B"
                description = ""
                rpc_host = "127.0.0.1"
                rpc_port = 0
                rpc_user = ""
                rpc_password = ""
                client_implementation = "bitcoincore"
            [[networks]]
            id = 1
            name = ""
            description = ""
            query_interval = 15
            first_tracked_height = 0
            visible_heights_from_tip = 0
            extra_hotspot_heights = 0
            network_type = "Regtest"

                [[networks.nodes]]
                id = 0
                name = "Node B"
                description = ""
                rpc_host = "127.0.0.1"
                rpc_port = 0
                rpc_user = ""
                rpc_password = ""
                client_implementation = "bitcoincore"
        "#,
        ) {
            // test OK, as we expect this to error
        } else {
            panic!("Test did not error!");
        }
    }

    #[test]
    fn esplora_backend_test() {
        match parse_config(
            r#"
            database_path = ""
            www_path = "./www"
            query_interval = 15
            address = "127.0.0.1:2323"
            rss_base_url = ""
            footer_html = ""

            [[networks]]
            id = 1
            name = ""
            description = ""
            query_interval = 15
            first_tracked_height = 0
            visible_heights_from_tip = 0
            extra_hotspot_heights = 0
            network_type = "Mainnet"

                [[networks.nodes]]
                id = 123
                name = "Esplora Node"
                description = "A test explora node"
                rpc_host = "https://esplora.example.org/api"
                client_implementation = "esplora"
        "#,
        ) {
            Ok(config) => {
                let network = &config.networks[0];
                let node = &network.nodes[0];
                let node_info = node.info();
                assert_eq!(node_info.name, "Esplora Node");
                assert_eq!(node_info.id, 123);
                assert_eq!(node_info.implementation, "esplora");
            }
            Err(e) => {
                panic!("Esplora backend config invalid: {}", e);
            }
        }
    }

    #[test]
    fn electrum_backend_test() {
        match parse_config(
            r#"
            database_path = ""
            www_path = "./www"
            query_interval = 15
            address = "127.0.0.1:2323"
            rss_base_url = ""
            footer_html = ""

            [[networks]]
            id = 1
            name = ""
            description = ""
            query_interval = 15
            first_tracked_height = 0
            visible_heights_from_tip = 0
            extra_hotspot_heights = 0
            network_type = "Mainnet"

                [[networks.nodes]]
                id = 421
                name = "Electrum"
                description = "electrum"
                rpc_host = "tcp://localhost"
                rpc_port = 1337
                client_implementation = "electrum"
        "#,
        ) {
            Ok(config) => {
                let network = &config.networks[0];
                let node = &network.nodes[0];
                let node_info = node.info();
                assert_eq!(node_info.name, "Electrum");
                assert_eq!(node_info.id, 421);
                assert_eq!(node_info.implementation, "electrum");
            }
            Err(e) => {
                panic!("Electrum backend config invalid: {}", e);
            }
        }
    }

    #[test]
    fn parses_visible_tip_window_and_hotspot_budget() {
        match parse_config(
            r#"
            database_path = ""
            query_interval = 15
            address = "127.0.0.1:2323"
            rss_base_url = ""

            [[networks]]
            id = 7
            name = "example"
            description = ""
            query_interval = 15
            first_tracked_height = 111
            visible_heights_from_tip = 222
            extra_hotspot_heights = 33
            network_type = "Mainnet"

                [[networks.nodes]]
                id = 1
                name = "Esplora Node"
                description = "test"
                rpc_host = "https://esplora.example.org/api"
                client_implementation = "esplora"
        "#,
        ) {
            Ok(config) => {
                let network = &config.networks[0];
                assert_eq!(network.visible_heights_from_tip, 222);
                assert_eq!(network.extra_hotspot_heights, 33);
                assert!(!network.disable_node_controls);
            }
            Err(e) => {
                panic!("new height fields should parse: {}", e);
            }
        }
    }

    #[test]
    fn parses_disable_node_controls_flag() {
        match parse_config(
            r#"
            database_path = ""
            query_interval = 15
            address = "127.0.0.1:2323"
            rss_base_url = ""

            [[networks]]
            id = 8
            name = "example"
            description = ""
            query_interval = 15
            first_tracked_height = 0
            visible_heights_from_tip = 10
            extra_hotspot_heights = 2
            network_type = "Regtest"
            disable_node_controls = true

                [[networks.nodes]]
                id = 1
                name = "Esplora Node"
                description = "test"
                rpc_host = "https://esplora.example.org/api"
                client_implementation = "esplora"
        "#,
        ) {
            Ok(config) => {
                let network = &config.networks[0];
                assert!(network.disable_node_controls);
            }
            Err(e) => {
                panic!("disable_node_controls=true should parse: {}", e);
            }
        }
    }

    #[test]
    fn missing_network_type_rejected() {
        match parse_config(
            r#"
            database_path = ""
            query_interval = 15
            address = "127.0.0.1:2323"
            rss_base_url = ""

            [[networks]]
            id = 1
            name = "missing-network-type"
            description = ""
            query_interval = 15
            first_tracked_height = 0
            visible_heights_from_tip = 10
            extra_hotspot_heights = 2

                [[networks.nodes]]
                id = 1
                name = "Esplora Node"
                description = "test"
                rpc_host = "https://esplora.example.org/api"
                client_implementation = "esplora"
        "#,
        ) {
            Ok(_) => panic!("missing network_type should fail parsing"),
            Err(ConfigError::TomlError(_)) => {}
            Err(e) => panic!(
                "expected TOML parse error for missing network_type, got {}",
                e
            ),
        }
    }

    #[test]
    fn missing_client_implementation_rejected() {
        match parse_config(
            r#"
            database_path = ""
            query_interval = 15
            address = "127.0.0.1:2323"
            rss_base_url = ""

            [[networks]]
            id = 1
            name = "missing-impl"
            description = ""
            query_interval = 15
            first_tracked_height = 0
            visible_heights_from_tip = 10
            extra_hotspot_heights = 2
            network_type = "Mainnet"

                [[networks.nodes]]
                id = 1
                name = "No Impl Node"
                description = "test"
                rpc_host = "127.0.0.1"
                rpc_port = 8332
                rpc_user = "user"
                rpc_password = "pass"
        "#,
        ) {
            Ok(_) => panic!("missing client_implementation should fail parsing"),
            Err(ConfigError::TomlError(_)) => {}
            Err(e) => panic!(
                "expected TOML parse error for missing client_implementation, got {}",
                e
            ),
        }
    }

    #[test]
    fn bitcoincore_and_other_nodes_parse_into_network_nodes() {
        let config = parse_config(
            r#"
            database_path = ""
            query_interval = 15
            address = "127.0.0.1:2323"
            rss_base_url = ""

            [[networks]]
            id = 1
            name = "controls"
            description = ""
            query_interval = 15
            first_tracked_height = 0
            visible_heights_from_tip = 10
            extra_hotspot_heights = 2
            network_type = "Regtest"

                [[networks.nodes]]
                id = 11
                name = "core"
                description = "bitcoincore node"
                rpc_host = "127.0.0.1"
                rpc_port = 18443
                rpc_user = "user"
                rpc_password = "pass"
                client_implementation = "bitcoincore"

                [[networks.nodes]]
                id = 12
                name = "esplora"
                description = "esplora node"
                rpc_host = "https://esplora.example.org/api"
                client_implementation = "esplora"
        "#,
        )
        .expect("config should parse");

        let network = &config.networks[0];
        assert_eq!(network.nodes.len(), 2);
        assert_eq!(network.nodes[0].info().id, 11);
        assert_eq!(network.nodes[0].info().implementation, "Bitcoin Core");
        assert_eq!(network.nodes[1].info().id, 12);
        assert_eq!(network.nodes[1].info().implementation, "esplora");
    }

}
