use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::modules::websocket::ServerMessage;
use crate::node_manager::NodeManager;

type ConnectionMap = Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<ServerMessage>>>>;

lazy_static::lazy_static! {
    static ref AGENT_CONNECTIONS: ConnectionMap = Arc::new(RwLock::new(HashMap::new()));
}

pub async fn register_agent_connection(node_id: String, tx: tokio::sync::mpsc::Sender<ServerMessage>) {
    let mut connections = AGENT_CONNECTIONS.write().await;
    connections.insert(node_id.clone(), tx);
    info!("Registered agent connection for node: {}", node_id);
}

pub async fn unregister_agent_connection(node_id: &str) {
    let mut connections = AGENT_CONNECTIONS.write().await;
    connections.remove(node_id);
    info!("Unregistered agent connection for node: {}", node_id);
}

pub async fn broadcast_configuration_update(node_manager: &Arc<NodeManager>) {
    let mut connections = AGENT_CONNECTIONS.write().await;
    let configs = node_manager.generate_configs().await;
    
    info!("Broadcasting configuration update to {} connected agents", connections.len());
    
    let mut failed_connections = Vec::new();
    
    for (node_id, tx) in connections.iter() {
        if let Some(config) = configs.get(node_id) {
            let update = ServerMessage::Update {
                listen: config.listen.clone(),
                peers: config.peers.clone(),
                allowed_public_keys: config.allowed_public_keys.clone(),
            };
            
            if let Err(e) = tx.send(update).await {
                warn!("Failed to send update to node {}: {}", node_id, e);
                failed_connections.push(node_id.clone());
            }
        } else {
            // Node was deleted, send empty configuration to disconnect agent gracefully
            let update = ServerMessage::Update {
                listen: vec![],
                peers: vec![],
                allowed_public_keys: vec![],
            };
            
            if let Err(e) = tx.send(update).await {
                warn!("Failed to send final update to deleted node {}: {}", node_id, e);
                failed_connections.push(node_id.clone());
            } else {
                info!("Sent final empty config to deleted node {}", node_id);
                failed_connections.push(node_id.clone());
            }
        }
    }
    
    // Remove failed connections
    for node_id in failed_connections {
        connections.remove(&node_id);
        info!("Removed failed connection for node: {}", node_id);
    }
}

pub async fn get_connected_agents_count() -> usize {
    AGENT_CONNECTIONS.read().await.len()
}