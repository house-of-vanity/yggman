use async_trait::async_trait;
use std::sync::Arc;
use crate::core::context::AppContext;
use crate::error::Result;

#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &str;
    
    async fn init(&mut self, context: Arc<AppContext>) -> Result<()>;
    
    async fn start(&self) -> Result<()>;
    
    async fn stop(&self) -> Result<()>;
}

pub struct ModuleManager {
    modules: Vec<Box<dyn Module>>,
    context: Arc<AppContext>,
}

impl ModuleManager {
    pub fn new(context: Arc<AppContext>) -> Self {
        Self {
            modules: Vec::new(),
            context,
        }
    }
    
    pub fn register(&mut self, module: Box<dyn Module>) {
        self.modules.push(module);
    }
    
    pub async fn init_all(&mut self) -> Result<()> {
        for module in &mut self.modules {
            tracing::info!("Initializing module: {}", module.name());
            module.init(self.context.clone()).await?;
        }
        Ok(())
    }
    
    pub async fn start_all(&self) -> Result<()> {
        for module in &self.modules {
            tracing::info!("Starting module: {}", module.name());
            module.start().await?;
        }
        Ok(())
    }
    
    pub async fn stop_all(&self) -> Result<()> {
        for module in self.modules.iter().rev() {
            tracing::info!("Stopping module: {}", module.name());
            module.stop().await?;
        }
        Ok(())
    }
}