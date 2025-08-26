use sea_orm::{DatabaseConnection, EntityTrait, QueryFilter, ColumnTrait};
use std::sync::Arc;

use crate::database::entities::settings::{Entity as SettingsEntity, ActiveModel};
use crate::error::AppError;
use crate::config::ConfigManager;

const LISTEN_TEMPLATE_KEY: &str = "listen_template";

#[derive(Clone)]
pub struct SettingsManager {
    db: Arc<DatabaseConnection>,
}

impl SettingsManager {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db: Arc::new(db),
        }
    }
    
    pub async fn get_listen_template(&self) -> Result<Vec<String>, AppError> {
        match SettingsEntity::find()
            .filter(crate::database::entities::settings::Column::Key.eq(LISTEN_TEMPLATE_KEY))
            .one(&*self.db)
            .await
            .map_err(|e| AppError::Config(format!("Database error: {}", e)))?
        {
            Some(setting) => {
                setting.parse_json_value::<Vec<String>>()
                    .map_err(|e| AppError::Config(format!("Failed to parse listen template: {}", e)))
            },
            None => {
                // Return default template if not found
                Ok(vec!["tcp://0.0.0.0:9001".to_string()])
            }
        }
    }
    
    pub async fn set_listen_template(&self, template: Vec<String>) -> Result<(), AppError> {
        // Check if setting already exists
        let existing = SettingsEntity::find()
            .filter(crate::database::entities::settings::Column::Key.eq(LISTEN_TEMPLATE_KEY))
            .one(&*self.db)
            .await
            .map_err(|e| AppError::Config(format!("Database error: {}", e)))?;
        
        if let Some(existing_setting) = existing {
            // Update existing setting
            let mut active_model: ActiveModel = existing_setting.into();
            active_model.update_value(&template)
                .map_err(|e| AppError::Config(format!("Failed to serialize template: {}", e)))?;
            
            SettingsEntity::update(active_model)
                .exec(&*self.db)
                .await
                .map_err(|e| AppError::Config(format!("Database error: {}", e)))?;
        } else {
            // Create new setting
            let active_model = ActiveModel::new(LISTEN_TEMPLATE_KEY.to_string(), &template)
                .map_err(|e| AppError::Config(format!("Failed to serialize template: {}", e)))?;
            
            SettingsEntity::insert(active_model)
                .exec(&*self.db)
                .await
                .map_err(|e| AppError::Config(format!("Database error: {}", e)))?;
        }
        
        tracing::info!("Listen template saved to database: {:?}", template);
        Ok(())
    }
    
    pub async fn initialize_defaults(&self) -> Result<(), AppError> {
        // Check if listen template exists, if not create default
        if SettingsEntity::find()
            .filter(crate::database::entities::settings::Column::Key.eq(LISTEN_TEMPLATE_KEY))
            .one(&*self.db)
            .await
            .map_err(|e| AppError::Config(format!("Database error: {}", e)))?
            .is_none()
        {
            let default_template = vec!["tcp://0.0.0.0:9001".to_string()];
            self.set_listen_template(default_template).await?;
            tracing::info!("Initialized default listen template");
        }
        
        Ok(())
    }
    
    pub async fn load_settings_to_config(&self, config_manager: &ConfigManager) -> Result<(), AppError> {
        // Load listen template from database and update config
        let template = self.get_listen_template().await?;
        config_manager.update_listen_template(template);
        tracing::info!("Loaded settings from database to config");
        Ok(())
    }
}