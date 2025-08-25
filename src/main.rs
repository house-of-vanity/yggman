mod cli;
mod config;
mod core;
mod database;
mod error;
mod modules;
mod node_manager;
mod yggdrasil;
mod websocket_state;

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let cli_args = cli::CliArgs::parse_args();
    
    // Load environment variables with YGGMAN_ prefix
    let env_config = cli::load_env_config()
        .unwrap_or_else(|_| cli::EnvConfig::default());
    
    // Initialize tracing with log level from CLI or env
    let log_level = if cli_args.debug {
        "debug"
    } else {
        &cli_args.log_level
    };
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("yggman={},info", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    tracing::info!("Starting yggman v{}", env!("CARGO_PKG_VERSION"));
    tracing::debug!("CLI args: {:?}", cli_args);
    tracing::debug!("Environment config: {:?}", env_config);
    
    // Load merged configuration
    let config = config::ConfigManager::load_merged_config(&cli_args, &env_config)?;
    tracing::info!("Configuration loaded from: CLI args, env vars, config file: {}", cli_args.config);
    tracing::info!("Database URL: {}", config.database.url);
    
    // Initialize database connection
    let db = database::create_connection(&config.database).await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;
    tracing::info!("Database connection established");
    
    // Run migrations
    database::migrate_database(&db).await
        .map_err(|e| anyhow::anyhow!("Failed to migrate database: {}", e))?;
    
    let mut app = core::app::Application::new(config);
    
    app.register_module(Box::new(modules::web::WebModule::new(db)));
    
    app.run().await?;
    
    Ok(())
}
