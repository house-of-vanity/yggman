use std::sync::Arc;
use crate::config::ConfigManager;

pub struct AppContext {
    pub config_manager: Arc<ConfigManager>,
}

impl AppContext {
    pub fn new(config_manager: Arc<ConfigManager>) -> Self {
        Self {
            config_manager,
        }
    }
}