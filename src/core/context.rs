use std::sync::Arc;
use crate::config::ConfigManager;
use crate::settings_manager::SettingsManager;

pub struct AppContext {
    pub config_manager: Arc<ConfigManager>,
    pub settings_manager: Arc<SettingsManager>,
}

impl AppContext {
    pub fn new(config_manager: Arc<ConfigManager>, settings_manager: Arc<SettingsManager>) -> Self {
        Self {
            config_manager,
            settings_manager,
        }
    }
}