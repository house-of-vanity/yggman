use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use arc_swap::ArcSwap;
use std::sync::Arc;
use crate::cli::{CliArgs, EnvConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    
    #[serde(default)]
    pub database: DatabaseConfig,
    
    #[serde(default)]
    pub nodes: NodesConfig,
    
    #[serde(default)]
    pub modules: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind_address: String,
    pub port: u16,
    pub workers: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connect_timeout: u64,
    pub acquire_timeout: u64,
    pub idle_timeout: u64,
    pub max_lifetime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodesConfig {
    pub max_peers_per_node: usize,
    pub topology_update_interval: u64,
    pub default_listen_endpoints: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            workers: 4,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://yggman.db".to_string(),
            max_connections: 10,
            connect_timeout: 30,
            acquire_timeout: 30,
            idle_timeout: 600,
            max_lifetime: 3600,
        }
    }
}

impl Default for NodesConfig {
    fn default() -> Self {
        Self {
            max_peers_per_node: 3,
            topology_update_interval: 60,
            default_listen_endpoints: vec!["tcp://0.0.0.0:9001".to_string()],
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            nodes: NodesConfig::default(),
            modules: HashMap::new(),
        }
    }
}

pub struct ConfigManager {
    config: Arc<ArcSwap<AppConfig>>,
}

impl ConfigManager {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(ArcSwap::from_pointee(config)),
        }
    }
    
    pub fn get(&self) -> Arc<AppConfig> {
        self.config.load_full()
    }
    
    pub fn update_listen_template(&self, new_template: Vec<String>) {
        let current = self.config.load_full();
        let mut new_config = current.as_ref().clone();
        new_config.nodes.default_listen_endpoints = new_template;
        
        self.config.store(Arc::new(new_config));
        tracing::info!("Listen template updated in memory");
    }
    
    
    /// Load configuration from multiple sources with precedence:
    /// CLI args > Environment variables > Config file > Defaults
    pub fn load_merged_config(cli_args: &CliArgs, env_config: &EnvConfig) -> Result<AppConfig, crate::error::AppError> {
        // Start with default config
        let mut config = AppConfig::default();
        
        // Load from config file if it exists
        if std::path::Path::new(&cli_args.config).exists() {
            let content = std::fs::read_to_string(&cli_args.config)
                .map_err(|e| crate::error::AppError::Config(format!("Failed to read config file: {}", e)))?;
            
            config = toml::from_str(&content)
                .map_err(|e| crate::error::AppError::Config(format!("Failed to parse config file: {}", e)))?;
        }
        
        // Override with environment variables
        if let Some(bind_address) = &env_config.server.bind_address {
            config.server.bind_address = bind_address.clone();
        }
        if let Some(port) = env_config.server.port {
            config.server.port = port;
        }
        if let Some(workers) = env_config.server.workers {
            config.server.workers = workers;
        }
        if let Some(db_url) = &env_config.database.url {
            config.database.url = db_url.clone();
        }
        if let Some(max_connections) = env_config.database.max_connections {
            config.database.max_connections = max_connections;
        }
        if let Some(max_peers) = env_config.nodes.max_peers_per_node {
            config.nodes.max_peers_per_node = max_peers;
        }
        if let Some(topology_update) = env_config.nodes.topology_update_interval {
            config.nodes.topology_update_interval = topology_update;
        }
        
        // Override with CLI arguments (highest priority)
        if let Some(bind_address) = &cli_args.bind_address {
            config.server.bind_address = bind_address.clone();
        }
        if let Some(port) = cli_args.port {
            config.server.port = port;
        }
        if let Some(workers) = cli_args.workers {
            config.server.workers = workers;
        }
        if let Some(db_url) = &cli_args.database_url {
            config.database.url = db_url.clone();
        }
        if let Some(max_connections) = cli_args.max_db_connections {
            config.database.max_connections = max_connections;
        }
        if let Some(max_peers) = cli_args.max_peers {
            config.nodes.max_peers_per_node = max_peers;
        }
        if let Some(topology_update) = cli_args.topology_update_interval {
            config.nodes.topology_update_interval = topology_update;
        }
        
        Ok(config)
    }
}