use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct YggdrasilConfig {
    #[serde(rename = "PrivateKey")]
    pub private_key: String,
    
    pub peers: Vec<String>,
    
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub listen: Vec<String>,
    
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allowed_public_keys: Vec<String>,
    
    #[serde(rename = "IfName")]
    pub if_name: String,
    
    #[serde(rename = "IfMTU")]
    pub if_mtu: u16,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_info_privacy: Option<bool>,
    
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub node_info: HashMap<String, serde_json::Value>,
}

impl Default for YggdrasilConfig {
    fn default() -> Self {
        Self {
            private_key: String::new(),
            peers: Vec::new(),
            listen: Vec::new(),
            allowed_public_keys: Vec::new(),
            if_name: "auto".to_string(),
            if_mtu: 65535,
            node_info_privacy: Some(false),
            node_info: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub private_key: String,
    pub listen: Vec<String>,
    pub addresses: Vec<String>, // Real IP addresses of the node
}