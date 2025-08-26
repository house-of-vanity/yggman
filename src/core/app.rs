use std::sync::Arc;
use crate::config::{AppConfig, ConfigManager};
use crate::core::context::AppContext;
use crate::core::module::ModuleManager;
use crate::error::Result;
use crate::settings_manager::SettingsManager;
use tokio::signal;

pub struct Application {
    module_manager: ModuleManager,
}

impl Application {
    pub fn new(config: AppConfig, settings_manager: SettingsManager) -> Self {
        let config_manager = Arc::new(ConfigManager::new(config));
        let context = Arc::new(AppContext::new(config_manager, Arc::new(settings_manager)));
        let module_manager = ModuleManager::new(context);
        
        Self {
            module_manager,
        }
    }
    
    pub fn new_with_managers(config_manager: ConfigManager, settings_manager: SettingsManager) -> Self {
        let context = Arc::new(AppContext::new(Arc::new(config_manager), Arc::new(settings_manager)));
        let module_manager = ModuleManager::new(context);
        
        Self {
            module_manager,
        }
    }
    
    pub fn register_module(&mut self, module: Box<dyn crate::core::module::Module>) {
        self.module_manager.register(module);
    }
    
    pub async fn run(mut self) -> Result<()> {
        tracing::info!("Starting application");
        
        self.module_manager.init_all().await?;
        
        self.module_manager.start_all().await?;
        
        tokio::select! {
            _ = signal::ctrl_c() => {
                tracing::info!("Received SIGINT, shutting down");
            }
        }
        
        self.shutdown().await?;
        
        Ok(())
    }
    
    async fn shutdown(self) -> Result<()> {
        tracing::info!("Shutting down application");
        
        self.module_manager.stop_all().await?;
        
        tracing::info!("Application shutdown complete");
        Ok(())
    }
}