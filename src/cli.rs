use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(
    name = "yggman",
    version = env!("CARGO_PKG_VERSION"),
    about = "Yggdrasil Network Control Plane Manager",
    long_about = "A centralized control plane for managing Yggdrasil node configurations and mesh topology"
)]
pub struct CliArgs {
    /// Configuration file path
    #[arg(short, long, default_value = "config.toml", env = "YGGMAN_CONFIG")]
    pub config: String,

    /// Server bind address
    #[arg(long, env = "YGGMAN_BIND_ADDRESS")]
    pub bind_address: Option<String>,

    /// Server port
    #[arg(short, long, env = "YGGMAN_PORT")]
    pub port: Option<u16>,

    /// Number of worker threads
    #[arg(long, env = "YGGMAN_WORKERS")]
    pub workers: Option<usize>,

    /// Database URL (SQLite: sqlite://path.db, PostgreSQL: postgresql://user:pass@host:port/db)
    #[arg(long, env = "YGGMAN_DATABASE_URL")]
    pub database_url: Option<String>,

    /// Maximum database connections
    #[arg(long, env = "YGGMAN_MAX_DB_CONNECTIONS")]
    pub max_db_connections: Option<u32>,

    /// Maximum peers per node
    #[arg(long, env = "YGGMAN_MAX_PEERS")]
    pub max_peers: Option<usize>,

    /// Topology update interval in seconds
    #[arg(long, env = "YGGMAN_TOPOLOGY_UPDATE_INTERVAL")]
    pub topology_update_interval: Option<u64>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", env = "YGGMAN_LOG_LEVEL")]
    pub log_level: String,

    /// Enable debug mode
    #[arg(long, env = "YGGMAN_DEBUG")]
    pub debug: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvConfig {
    #[serde(default)]
    pub server: EnvServerConfig,
    
    #[serde(default)]
    pub database: EnvDatabaseConfig,
    
    #[serde(default)]
    pub nodes: EnvNodesConfig,
    
    #[serde(default)]
    pub log_level: Option<String>,
    
    #[serde(default)]
    pub debug: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvServerConfig {
    pub bind_address: Option<String>,
    pub port: Option<u16>,
    pub workers: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvDatabaseConfig {
    pub url: Option<String>,
    pub max_connections: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvNodesConfig {
    pub max_peers_per_node: Option<usize>,
    pub topology_update_interval: Option<u64>,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            server: EnvServerConfig::default(),
            database: EnvDatabaseConfig::default(),
            nodes: EnvNodesConfig::default(),
            log_level: None,
            debug: None,
        }
    }
}

impl Default for EnvServerConfig {
    fn default() -> Self {
        Self {
            bind_address: None,
            port: None,
            workers: None,
        }
    }
}

impl Default for EnvDatabaseConfig {
    fn default() -> Self {
        Self {
            url: None,
            max_connections: None,
        }
    }
}

impl Default for EnvNodesConfig {
    fn default() -> Self {
        Self {
            max_peers_per_node: None,
            topology_update_interval: None,
        }
    }
}

impl CliArgs {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

/// Load environment variables with YGGMAN_ prefix
pub fn load_env_config() -> Result<EnvConfig, envy::Error> {
    envy::prefixed("YGGMAN_").from_env::<EnvConfig>()
}

