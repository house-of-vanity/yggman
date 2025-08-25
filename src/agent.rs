use anyhow::Result;
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use serde::{Deserialize, Serialize};
use std::time::Duration;
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
    info!("Connecting to control plane: {}", args.server);

    // Main loop with reconnection logic
    loop {
        match run_agent(&args).await {
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

async fn run_agent(args: &Args) -> Result<()> {
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

    // Main message loop
    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ServerMessage>(&text) {
                            Ok(server_msg) => handle_server_message(server_msg).await?,
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
        }
    }

    Ok(())
}

async fn handle_server_message(msg: ServerMessage) -> Result<()> {
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
            
            // TODO: Apply configuration to Yggdrasil
            info!("Configuration received (not yet applied)");
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
            
            // TODO: Apply configuration update to Yggdrasil
            info!("Configuration update received (not yet applied)");
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