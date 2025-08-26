use anyhow::{Result, anyhow};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use std::path::Path;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn, debug};

#[derive(Parser, Debug)]
#[command(
    name = "yggman-agent",
    about = "Yggdrasil network agent for automatic node configuration"
)]
struct Args {
    /// Control plane server URL (e.g., ws://localhost:8080/ws/agent)
    #[arg(short, long)]
    server: String,

    /// Node name (optional, will use hostname if not provided)
    #[arg(short, long)]
    name: Option<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Reconnect interval in seconds
    #[arg(long, default_value = "5")]
    reconnect_interval: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AgentMessage {
    Register {
        name: String,
        addresses: Vec<String>,
    },
    Heartbeat,
    UpdateAddresses {
        addresses: Vec<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ServerMessage {
    Config {
        node_id: String,
        private_key: String,
        listen: Vec<String>,
        peers: Vec<String>,
        allowed_public_keys: Vec<String>,
    },
    Update {
        peers: Vec<String>,
        allowed_public_keys: Vec<String>,
    },
    Error {
        message: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(args.log_level.parse::<tracing::Level>()?)
        .init();

    info!("Starting yggman-agent v{}", env!("CARGO_PKG_VERSION"));
    
    // Check for yggdrasil config file
    let ygg_config_path = find_yggdrasil_config().ok_or_else(|| {
        anyhow!("Yggdrasil config file not found. Please ensure yggdrasil.conf exists at /etc/yggdrasil.conf or /etc/yggdrasil/yggdrasil.conf")
    })?;
    info!("Found Yggdrasil config at: {}", ygg_config_path);
    
    info!("Connecting to control plane: {}", args.server);

    // Main loop with reconnection logic
    loop {
        match run_agent(&args, &ygg_config_path).await {
            Ok(_) => {
                info!("Agent connection closed normally");
            }
            Err(e) => {
                error!("Agent error: {}", e);
            }
        }

        info!(
            "Reconnecting in {} seconds...",
            args.reconnect_interval
        );
        sleep(Duration::from_secs(args.reconnect_interval)).await;
    }
}

async fn run_agent(args: &Args, ygg_config_path: &str) -> Result<()> {
    // Get node name
    let node_name = args.name.clone().unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    });

    // Discover network interfaces
    let addresses = discover_addresses()?;
    info!("Discovered addresses: {:?}", addresses);

    // Connect to WebSocket
    let (ws_stream, _) = connect_async(&args.server).await?;
    info!("Connected to control plane");

    let (mut write, mut read) = ws_stream.split();

    // Send registration message
    let register_msg = AgentMessage::Register {
        name: node_name.clone(),
        addresses: addresses.clone(),
    };
    
    let json = serde_json::to_string(&register_msg)?;
    write.send(Message::Text(json)).await?;
    info!("Sent registration for node: {}", node_name);

    // Spawn heartbeat task
    let (heartbeat_tx, mut heartbeat_rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if heartbeat_tx.send(()).await.is_err() {
                break;
            }
        }
    });
    
    // Spawn address scanning task
    let (address_scan_tx, mut address_scan_rx) = tokio::sync::mpsc::channel(1);
    let current_addresses = Arc::new(tokio::sync::RwLock::new(addresses.clone()));
    let current_addresses_clone = current_addresses.clone();
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60)); // Scan every minute
        loop {
            interval.tick().await;
            
            match discover_addresses() {
                Ok(new_addresses) => {
                    let mut current = current_addresses_clone.write().await;
                    
                    // Check if addresses have changed
                    if *current != new_addresses {
                        info!("Address change detected: {:?} -> {:?}", *current, new_addresses);
                        *current = new_addresses.clone();
                        
                        if address_scan_tx.send(new_addresses).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to scan addresses: {}", e);
                }
            }
        }
    });

    // Main message loop
    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ServerMessage>(&text) {
                            Ok(server_msg) => handle_server_message(server_msg, ygg_config_path).await?,
                            Err(e) => warn!("Failed to parse server message: {}", e),
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("Server closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        info!("WebSocket stream ended");
                        break;
                    }
                    _ => {}
                }
            }
            _ = heartbeat_rx.recv() => {
                let heartbeat = serde_json::to_string(&AgentMessage::Heartbeat)?;
                if let Err(e) = write.send(Message::Text(heartbeat)).await {
                    error!("Failed to send heartbeat: {}", e);
                    break;
                }
                debug!("Sent heartbeat");
            }
            Some(new_addresses) = address_scan_rx.recv() => {
                let update_msg = AgentMessage::UpdateAddresses {
                    addresses: new_addresses,
                };
                let json = serde_json::to_string(&update_msg)?;
                if let Err(e) = write.send(Message::Text(json)).await {
                    error!("Failed to send address update: {}", e);
                    break;
                }
                info!("Sent address update to control plane");
            }
        }
    }

    Ok(())
}

async fn handle_server_message(msg: ServerMessage, ygg_config_path: &str) -> Result<()> {
    match msg {
        ServerMessage::Config {
            node_id,
            private_key,
            listen,
            peers,
            allowed_public_keys,
        } => {
            info!("Received initial configuration:");
            info!("  Node ID: {}", node_id);
            info!("  Private Key: {}...", &private_key[..16]);
            info!("  Listen endpoints: {:?}", listen);
            info!("  Peers: {} configured", peers.len());
            for peer in &peers {
                debug!("    - {}", peer);
            }
            info!("  Allowed keys: {} configured", allowed_public_keys.len());
            
            // Apply configuration to Yggdrasil
            match write_yggdrasil_config(ygg_config_path, &private_key, &listen, &peers, &allowed_public_keys).await {
                Ok(_) => info!("Configuration successfully applied to {}", ygg_config_path),
                Err(e) => error!("Failed to write Yggdrasil config: {}", e),
            }
        }
        ServerMessage::Update {
            peers,
            allowed_public_keys,
        } => {
            info!("Received configuration update:");
            info!("  Updated peers: {} configured", peers.len());
            for peer in &peers {
                debug!("    - {}", peer);
            }
            info!("  Updated allowed keys: {} configured", allowed_public_keys.len());
            
            // Apply configuration update to Yggdrasil 
            // For updates we need to read current config and update only peers/allowed keys
            match update_yggdrasil_config(ygg_config_path, &peers, &allowed_public_keys).await {
                Ok(_) => info!("Configuration update successfully applied to {}", ygg_config_path),
                Err(e) => error!("Failed to update Yggdrasil config: {}", e),
            }
        }
        ServerMessage::Error { message } => {
            error!("Server error: {}", message);
        }
    }
    
    Ok(())
}

fn discover_addresses() -> Result<Vec<String>> {
    let interfaces = NetworkInterface::show()?;
    let mut addresses = Vec::new();

    for interface in interfaces {
        // Skip loopback and down interfaces
        if interface.name.starts_with("lo") {
            continue;
        }

        for addr in interface.addr {
            match addr {
                network_interface::Addr::V4(v4) => {
                    let ip = v4.ip.to_string();
                    // Skip link-local and private addresses for now
                    // In production, you might want to be more selective
                    if !ip.starts_with("127.") && !ip.starts_with("169.254.") {
                        addresses.push(ip);
                    }
                }
                network_interface::Addr::V6(v6) => {
                    let ip = v6.ip.to_string();
                    // Skip link-local IPv6
                    if !ip.starts_with("fe80:") && !ip.starts_with("::1") {
                        addresses.push(ip);
                    }
                }
            }
        }
    }

    // If no addresses found, return empty vec (will use localhost)
    Ok(addresses)
}

fn find_yggdrasil_config() -> Option<String> {
    let possible_paths = vec![
        "/etc/yggdrasil.conf",
        "/etc/yggdrasil/yggdrasil.conf",
    ];
    
    for path in possible_paths {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    
    None
}

async fn write_yggdrasil_config(
    config_path: &str,
    private_key: &str,
    listen: &[String],
    peers: &[String], 
    allowed_public_keys: &[String]
) -> Result<()> {
    use serde_json::json;
    
    let config = json!({
        "PrivateKey": private_key,
        "Listen": listen,
        "Peers": peers,
        "AllowedPublicKeys": allowed_public_keys,
        "InterfacePeers": {},
        "NodeInfo": {},
        "NodeInfoPrivacy": false
    });
    
    let config_json = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(config_path, config_json).await?;
    
    info!("Yggdrasil configuration written to {}", config_path);
    Ok(())
}

async fn update_yggdrasil_config(
    config_path: &str,
    peers: &[String],
    allowed_public_keys: &[String]
) -> Result<()> {
    // Read current config
    let current_config = tokio::fs::read_to_string(config_path).await?;
    let mut config: serde_json::Value = serde_json::from_str(&current_config)?;
    
    // Update only peers and allowed public keys
    config["Peers"] = serde_json::json!(peers);
    config["AllowedPublicKeys"] = serde_json::json!(allowed_public_keys);
    
    // Write updated config back
    let updated_config = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(config_path, updated_config).await?;
    
    info!("Yggdrasil configuration updated in {}", config_path);
    Ok(())
}