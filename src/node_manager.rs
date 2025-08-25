use crate::yggdrasil::{Node, YggdrasilConfig};
use crate::database::entities::node as node_entity;
use ed25519_dalek::{SigningKey, VerifyingKey};
use sea_orm::{DatabaseConnection, EntityTrait, ActiveModelTrait};
use std::collections::HashMap;

pub struct NodeManager {
    db: DatabaseConnection,
}

impl NodeManager {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
    
    pub async fn add_node(&self, name: String, listen: Vec<String>, addresses: Vec<String>) -> Result<(), crate::error::AppError> {
        let signing_key = SigningKey::from_bytes(&rand::random());
        let verifying_key: VerifyingKey = signing_key.verifying_key();
        
        let private_seed = signing_key.to_bytes();
        let public_key_bytes = verifying_key.to_bytes();
        
        // Yggdrasil expects a 64-byte private key (32-byte seed + 32-byte public key)
        let mut full_private_key = Vec::with_capacity(64);
        full_private_key.extend_from_slice(&private_seed);
        full_private_key.extend_from_slice(&public_key_bytes);
        
        let private_key = hex::encode(full_private_key);
        let public_key = hex::encode(public_key_bytes);
        
        let node = Node {
            id: format!("node-{}", uuid_simple()),
            name: name.clone(),
            public_key: public_key.clone(),
            private_key,
            listen,
            addresses,
        };
        
        // Save to database
        let active_model = node_entity::ActiveModel::from(&node);
        active_model.insert(&self.db).await
            .map_err(|e| crate::error::AppError::Config(format!("Database error: {}", e)))?;
        
        Ok(())
    }
    
    pub async fn update_node(&self, node_id: &str, name: String, listen: Vec<String>, addresses: Vec<String>) -> Result<(), crate::error::AppError> {
        // Check if node exists
        let existing_node = node_entity::Entity::find_by_id(node_id)
            .one(&self.db)
            .await
            .map_err(|e| crate::error::AppError::Config(format!("Database error: {}", e)))?;
            
        if existing_node.is_none() {
            return Err(crate::error::AppError::Config("Node not found".to_string()));
        }
        
        // Update the node
        let mut active_model: node_entity::ActiveModel = existing_node.unwrap().into();
        active_model.name = sea_orm::Set(name);
        active_model.listen = sea_orm::Set(serde_json::to_string(&listen).unwrap_or_default());
        active_model.addresses = sea_orm::Set(serde_json::to_string(&addresses).unwrap_or_default());
        
        active_model.update(&self.db).await
            .map_err(|e| crate::error::AppError::Config(format!("Database error: {}", e)))?;
            
        Ok(())
    }
    
    pub async fn remove_node(&self, node_id: &str) -> Result<(), crate::error::AppError> {
        let result = node_entity::Entity::delete_by_id(node_id)
            .exec(&self.db)
            .await
            .map_err(|e| crate::error::AppError::Config(format!("Database error: {}", e)))?;
            
        if result.rows_affected == 0 {
            return Err(crate::error::AppError::Config("Node not found".to_string()));
        }
        
        Ok(())
    }
    
    pub async fn get_node_by_id(&self, node_id: &str) -> Option<Node> {
        match node_entity::Entity::find_by_id(node_id).one(&self.db).await {
            Ok(Some(model)) => Some(Node::from(model)),
            _ => None,
        }
    }
    
    pub async fn get_node_by_name(&self, name: &str) -> Option<Node> {
        use sea_orm::{ColumnTrait, QueryFilter};
        match node_entity::Entity::find()
            .filter(node_entity::Column::Name.eq(name))
            .one(&self.db).await {
            Ok(Some(model)) => Some(Node::from(model)),
            _ => None,
        }
    }
    
    
    pub async fn get_all_nodes(&self) -> Vec<Node> {
        match node_entity::Entity::find().all(&self.db).await {
            Ok(models) => models.into_iter().map(Node::from).collect(),
            Err(e) => {
                tracing::error!("Failed to fetch nodes from database: {}", e);
                Vec::new()
            }
        }
    }
    
    pub async fn generate_configs(&self) -> HashMap<String, YggdrasilConfig> {
        let nodes = self.get_all_nodes().await;
        let mut configs = HashMap::new();
        
        let all_public_keys: Vec<String> = nodes
            .iter()
            .map(|n| n.public_key.clone())
            .collect();
        
        for node in &nodes {
            let mut config = YggdrasilConfig::default();
            
            config.private_key = node.private_key.clone();
            config.listen = node.listen.clone();
            
            let mut other_keys = all_public_keys.clone();
            other_keys.retain(|k| k != &node.public_key);
            config.allowed_public_keys = other_keys;
            
            // Build peers from other nodes' listen endpoints
            let mut peers: Vec<String> = Vec::new();
            for other_node in &nodes {
                if other_node.id != node.id {
                    // For each listen endpoint, create peers for all node addresses
                    for listen_addr in &other_node.listen {
                        // If no addresses provided, use localhost
                        let addresses_to_use = if other_node.addresses.is_empty() {
                            vec!["127.0.0.1".to_string()]
                        } else {
                            other_node.addresses.clone()
                        };
                        
                        for address in &addresses_to_use {
                            if let Some(peer_addr) = convert_listen_to_peer_with_address(listen_addr, &other_node.public_key, address) {
                                peers.push(peer_addr);
                            }
                        }
                    }
                }
            }
            config.peers = peers;
            
            let mut node_info = HashMap::new();
            node_info.insert("name".to_string(), serde_json::Value::String(node.name.clone()));
            config.node_info = node_info;
            
            configs.insert(node.id.clone(), config);
        }
        
        configs
    }
    
}

fn uuid_simple() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..16).map(|_| rng.r#gen()).collect();
    hex::encode(bytes)
}

fn convert_listen_to_peer_with_address(listen_addr: &str, public_key: &str, address: &str) -> Option<String> {
    // Parse the listen address and convert to peer format
    // Listen format: tcp://[::]:1234 or tcp://0.0.0.0:1234
    // Peer format: tcp://REAL_IP:1234?key=PUBLIC_KEY
    
    if listen_addr.contains("unix://") {
        // Unix sockets are local only, skip
        return None;
    }
    
    // Extract protocol and port
    let parts: Vec<&str> = listen_addr.split("://").collect();
    if parts.len() != 2 {
        return None;
    }
    
    let protocol = parts[0];
    let addr_part = parts[1];
    
    // Extract port from address
    let port = if addr_part.contains("]:") {
        // IPv6 format [::]:port
        addr_part.split("]:").nth(1)
    } else {
        // IPv4 format 0.0.0.0:port
        addr_part.split(':').nth(1)
    };
    
    let port = port?;
    
    // Check for query parameters in the original listen address
    let (port_clean, params) = if port.contains('?') {
        let parts: Vec<&str> = port.split('?').collect();
        (parts[0], Some(parts[1]))
    } else {
        (port, None)
    };
    
    // Build the peer address with the specified IP
    let mut peer_addr = format!("{}://{}:{}?key={}", protocol, address, port_clean, public_key);
    
    // Add any additional parameters from the listen address
    if let Some(params) = params {
        peer_addr.push('&');
        peer_addr.push_str(params);
    }
    
    Some(peer_addr)
}

