use async_trait::async_trait;
use axum::{
    extract::{State, Path, WebSocketUpgrade},
    http::StatusCode,
    response::{Html, Json, Response},
    routing::{get, post, put, delete},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use sea_orm::DatabaseConnection;

use crate::core::context::AppContext;
use crate::core::module::Module;
use crate::error::Result;
use crate::node_manager::NodeManager;
use crate::yggdrasil::{Node, YggdrasilConfig};

pub struct WebModule {
    name: String,
    context: Option<Arc<AppContext>>,
    node_manager: Arc<NodeManager>,
}

impl WebModule {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            name: "web".to_string(),
            context: None,
            node_manager: Arc::new(NodeManager::new(db)),
        }
    }
}

#[async_trait]
impl Module for WebModule {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn init(&mut self, context: Arc<AppContext>) -> Result<()> {
        self.context = Some(context);
        tracing::info!("Web module initialized");
        Ok(())
    }
    
    async fn start(&self) -> Result<()> {
        let context = self.context.as_ref().unwrap();
        let config = context.config_manager.get();
        let port = config.server.port;
        
        tracing::info!("Starting web server on port {}", port);
        
        let node_manager = self.node_manager.clone();
        
        let app = Router::new()
            .route("/", get(index_handler))
            .route("/api/nodes", get(get_nodes_handler))
            .route("/api/nodes", post(add_node_handler))
            .route("/api/nodes/:id", get(get_node_handler))
            .route("/api/nodes/:id", put(update_node_handler))
            .route("/api/nodes/:id", delete(delete_node_handler))
            .route("/api/configs", get(get_configs_handler))
            .route("/api/nodes/:id/config", get(get_node_config_handler))
            .route("/ws/agent", get(ws_agent_handler))
            .layer(CorsLayer::permissive())
            .with_state(node_manager);
        
        let bind_addr = format!("{}:{}", config.server.bind_address, port);
        let listener = tokio::net::TcpListener::bind(&bind_addr)
            .await
            .map_err(|e| crate::error::AppError::Io(e))?;
            
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("Failed to run web server");
        });
        
        Ok(())
    }
    
    async fn stop(&self) -> Result<()> {
        tracing::info!("Web module stopped");
        Ok(())
    }
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

#[derive(serde::Serialize)]
struct NodesResponse {
    nodes: Vec<Node>,
}

async fn get_nodes_handler(
    State(node_manager): State<Arc<NodeManager>>,
) -> Json<NodesResponse> {
    let nodes = node_manager.get_all_nodes().await;
    Json(NodesResponse { nodes })
}

#[derive(serde::Deserialize)]
struct AddNodeRequest {
    name: String,
    listen: Vec<String>,
    addresses: Vec<String>,
}

#[derive(serde::Serialize)]
struct AddNodeResponse {
    success: bool,
    message: String,
}

async fn add_node_handler(
    State(node_manager): State<Arc<NodeManager>>,
    Json(payload): Json<AddNodeRequest>,
) -> Json<AddNodeResponse> {
    match node_manager.add_node(payload.name, payload.listen, payload.addresses).await {
        Ok(_) => {
            // Broadcast update to all connected agents
            crate::websocket_state::broadcast_configuration_update(&node_manager).await;
            
            Json(AddNodeResponse {
                success: true,
                message: "Node added successfully".to_string(),
            })
        }
        Err(e) => Json(AddNodeResponse {
            success: false,
            message: format!("Failed to add node: {}", e),
        }),
    }
}

#[derive(serde::Serialize)]
struct ConfigsResponse {
    configs: Vec<NodeConfig>,
}

#[derive(serde::Serialize)]
struct NodeConfig {
    node_id: String,
    node_name: String,
    node_addresses: Vec<String>,
    config: YggdrasilConfig,
}

async fn get_configs_handler(
    State(node_manager): State<Arc<NodeManager>>,
) -> Json<ConfigsResponse> {
    let nodes = node_manager.get_all_nodes().await;
    let configs_map = node_manager.generate_configs().await;
    
    let mut configs = Vec::new();
    for node in nodes {
        if let Some(config) = configs_map.get(&node.id) {
            configs.push(NodeConfig {
                node_id: node.id.clone(),
                node_name: node.name.clone(),
                node_addresses: node.addresses.clone(),
                config: config.clone(),
            });
        }
    }
    
    Json(ConfigsResponse { configs })
}

// Get single node handler
async fn get_node_handler(
    State(node_manager): State<Arc<NodeManager>>,
    Path(node_id): Path<String>,
) -> std::result::Result<Json<Node>, StatusCode> {
    match node_manager.get_node_by_id(&node_id).await {
        Some(node) => Ok(Json(node)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

// Update node handler
async fn update_node_handler(
    State(node_manager): State<Arc<NodeManager>>,
    Path(node_id): Path<String>,
    Json(payload): Json<AddNodeRequest>,
) -> std::result::Result<Json<AddNodeResponse>, StatusCode> {
    match node_manager.update_node(&node_id, payload.name, payload.listen, payload.addresses).await {
        Ok(_) => {
            // Broadcast update to all connected agents
            crate::websocket_state::broadcast_configuration_update(&node_manager).await;
            
            Ok(Json(AddNodeResponse {
                success: true,
                message: "Node updated successfully".to_string(),
            }))
        }
        Err(e) => {
            if e.to_string().contains("Node not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(AddNodeResponse {
                    success: false,
                    message: format!("Failed to update node: {}", e),
                }))
            }
        }
    }
}

// Delete node handler
async fn delete_node_handler(
    State(node_manager): State<Arc<NodeManager>>,
    Path(node_id): Path<String>,
) -> std::result::Result<Json<AddNodeResponse>, StatusCode> {
    match node_manager.remove_node(&node_id).await {
        Ok(_) => {
            // Broadcast update to all connected agents
            crate::websocket_state::broadcast_configuration_update(&node_manager).await;
            
            Ok(Json(AddNodeResponse {
                success: true,
                message: "Node deleted successfully".to_string(),
            }))
        }
        Err(e) => {
            if e.to_string().contains("Node not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(AddNodeResponse {
                    success: false,
                    message: format!("Failed to delete node: {}", e),
                }))
            }
        }
    }
}

// Get node configuration for agent
async fn get_node_config_handler(
    State(node_manager): State<Arc<NodeManager>>,
    Path(node_id): Path<String>,
) -> std::result::Result<Json<NodeConfig>, StatusCode> {
    // Get the node
    let node = match node_manager.get_node_by_id(&node_id).await {
        Some(node) => node,
        None => return Err(StatusCode::NOT_FOUND),
    };
    
    // Generate configurations for all nodes
    let configs_map = node_manager.generate_configs().await;
    
    // Get config for this specific node
    match configs_map.get(&node_id) {
        Some(config) => Ok(Json(NodeConfig {
            node_id: node.id.clone(),
            node_name: node.name.clone(),
            node_addresses: node.addresses.clone(),
            config: config.clone(),
        })),
        None => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// WebSocket handler for agents
async fn ws_agent_handler(
    ws: WebSocketUpgrade,
    State(node_manager): State<Arc<NodeManager>>,
) -> Response {
    ws.on_upgrade(move |socket| crate::modules::websocket::handle_agent_socket(socket, node_manager))
}