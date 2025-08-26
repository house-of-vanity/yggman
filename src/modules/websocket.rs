use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::node_manager::NodeManager;
use crate::core::context::AppContext;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentMessage {
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
pub enum ServerMessage {
    Config {
        node_id: String,
        private_key: String,
        listen: Vec<String>,
        peers: Vec<String>,
        allowed_public_keys: Vec<String>,
    },
    Update {
        listen: Vec<String>,
        peers: Vec<String>,
        allowed_public_keys: Vec<String>,
    },
    Error {
        message: String,
    },
}


pub async fn handle_agent_socket(
    socket: WebSocket,
    node_manager: Arc<NodeManager>,
    context: Arc<AppContext>,
) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ServerMessage>(100);
    
    let mut node_id: Option<String> = None;

    // Spawn task to forward messages from channel to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        if let Ok(Message::Text(text)) = msg {
            match serde_json::from_str::<AgentMessage>(&text) {
                Ok(agent_msg) => {
                    match agent_msg {
                        AgentMessage::Register { name, addresses } => {
                            info!("Agent registration: {} with addresses {:?}", name, addresses);
                            
                            // Get default endpoints from settings database
                            let default_listen = match context.settings_manager.get_listen_template().await {
                                Ok(template) => template,
                                Err(e) => {
                                    error!("Failed to get listen template from database: {}", e);
                                    vec!["tcp://0.0.0.0:9001".to_string()] // fallback
                                }
                            };
                            
                            // Check if node already exists
                            let node = if let Some(existing_node) = node_manager.get_node_by_name(&name).await {
                                info!("Reusing existing node: {} ({})", existing_node.name, existing_node.id);
                                // Update addresses for existing node
                                match node_manager.update_node(&existing_node.id, name.clone(), default_listen.clone(), addresses).await {
                                    Ok(_) => {
                                        // Get the updated node
                                        node_manager.get_node_by_id(&existing_node.id).await
                                    }
                                    Err(e) => {
                                        warn!("Failed to update existing node addresses: {}", e);
                                        Some(existing_node)
                                    }
                                }
                            } else {
                                // Create new node
                                info!("Creating new node: {}", name);
                                match node_manager.add_node(name.clone(), default_listen.clone(), addresses).await {
                                    Ok(_) => {
                                        // Get the newly created node
                                        node_manager.get_node_by_name(&name).await
                                    }
                                    Err(e) => {
                                        let error_msg = ServerMessage::Error {
                                            message: format!("Failed to register node: {}", e),
                                        };
                                        let _ = tx.send(error_msg).await;
                                        None
                                    }
                                }
                            };
                            
                            if let Some(node) = node {
                                node_id = Some(node.id.clone());
                                
                                // Register connection
                                crate::websocket_state::register_agent_connection(node.id.clone(), tx.clone()).await;
                                
                                // Generate config for this node
                                let configs = node_manager.generate_configs().await;
                                if let Some(config) = configs.get(&node.id) {
                                    let peers: Vec<String> = config.peers.clone();
                                    let allowed_keys: Vec<String> = config.allowed_public_keys.clone();
                                    
                                    let response = ServerMessage::Config {
                                        node_id: node.id.clone(),
                                        private_key: node.private_key.clone(),
                                        listen: default_listen,
                                        peers,
                                        allowed_public_keys: allowed_keys,
                                    };
                                    
                                    if let Err(e) = tx.send(response).await {
                                        error!("Failed to send config to agent: {}", e);
                                    }
                                    
                                    // Notify other agents about node connection
                                    crate::websocket_state::broadcast_configuration_update(&node_manager).await;
                                }
                            }
                        }
                        AgentMessage::Heartbeat => {
                            debug!("Heartbeat from {:?}", node_id);
                        }
                        AgentMessage::UpdateAddresses { addresses } => {
                            if let Some(id) = &node_id {
                                info!("Address update for {}: {:?}", id, addresses);
                                
                                // Get current node information
                                if let Some(current_node) = node_manager.get_node_by_id(id).await {
                                    // Update node with new addresses
                                    match node_manager.update_node(
                                        id, 
                                        current_node.name.clone(), 
                                        current_node.listen.clone(),
                                        addresses
                                    ).await {
                                        Ok(_) => {
                                            info!("Updated addresses for node {}", id);
                                            // Broadcast configuration update to all agents
                                            crate::websocket_state::broadcast_configuration_update(&node_manager).await;
                                        }
                                        Err(e) => {
                                            error!("Failed to update addresses for node {}: {}", id, e);
                                        }
                                    }
                                } else {
                                    warn!("Cannot update addresses for unknown node: {}", id);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to parse agent message: {}", e);
                }
            }
        }
    }

    // Clean up
    if let Some(id) = node_id {
        crate::websocket_state::unregister_agent_connection(&id).await;
        info!("Agent {} disconnected", id);
    }

    // Abort send task
    send_task.abort();
}


