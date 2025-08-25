use async_trait::async_trait;
use std::sync::Arc;
use crate::core::context::AppContext;
use crate::core::module::Module;
use crate::error::Result;

pub struct ExampleModule {
    name: String,
    context: Option<Arc<AppContext>>,
}


#[async_trait]
impl Module for ExampleModule {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn init(&mut self, context: Arc<AppContext>) -> Result<()> {
        self.context = Some(context);
        tracing::debug!("Example module initialized");
        Ok(())
    }
    
    async fn start(&self) -> Result<()> {
        tracing::debug!("Example module started");
        
        if let Some(ctx) = &self.context {
            let config = ctx.config_manager.get();
            tracing::debug!("Current config: {:?}", config);
        }
        
        Ok(())
    }
    
    async fn stop(&self) -> Result<()> {
        tracing::debug!("Example module stopped");
        Ok(())
    }
}