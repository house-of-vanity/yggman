use sea_orm::{Database, DatabaseConnection, DbErr, ConnectionTrait};
use sea_orm::{Schema, DbBackend, Statement};
use migration::prelude::{SqliteQueryBuilder, PostgresQueryBuilder, MysqlQueryBuilder};
use std::time::Duration;
use std::path::Path;
use crate::config::DatabaseConfig;

pub async fn create_connection(config: &DatabaseConfig) -> Result<DatabaseConnection, DbErr> {
    // Create SQLite database file if it doesn't exist
    if config.url.starts_with("sqlite://") {
        let db_path = config.url.strip_prefix("sqlite://").unwrap_or(&config.url);
        
        // Create parent directories if they don't exist
        if let Some(parent) = Path::new(db_path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| DbErr::Custom(format!("Failed to create database directory: {}", e)))?;
            }
        }
        
        // Create empty database file if it doesn't exist
        if !Path::new(db_path).exists() {
            std::fs::File::create(db_path)
                .map_err(|e| DbErr::Custom(format!("Failed to create database file: {}", e)))?;
            tracing::info!("Created SQLite database file: {}", db_path);
        }
    }
    
    let mut options = sea_orm::ConnectOptions::new(&config.url);
    
    options
        .max_connections(config.max_connections)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(config.connect_timeout))
        .acquire_timeout(Duration::from_secs(config.acquire_timeout))
        .idle_timeout(Duration::from_secs(config.idle_timeout))
        .max_lifetime(Duration::from_secs(config.max_lifetime))
        .sqlx_logging(true)
        .sqlx_logging_level(tracing::log::LevelFilter::Debug);

    Database::connect(options).await
}

pub async fn migrate_database(db: &DatabaseConnection) -> Result<(), DbErr> {
    // Get the database backend
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);
    
    // Create nodes table if it doesn't exist
    let mut create_nodes_stmt = schema.create_table_from_entity(crate::database::entities::node::Entity);
    
    // Convert to SQL
    let nodes_sql = match backend {
        DbBackend::Sqlite => create_nodes_stmt.if_not_exists().to_string(SqliteQueryBuilder),
        DbBackend::Postgres => create_nodes_stmt.if_not_exists().to_string(PostgresQueryBuilder),
        DbBackend::MySql => create_nodes_stmt.if_not_exists().to_string(MysqlQueryBuilder),
    };
    
    // Execute the statement
    db.execute(Statement::from_string(backend, nodes_sql)).await?;
    
    // Create settings table if it doesn't exist
    let mut create_settings_stmt = schema.create_table_from_entity(crate::database::entities::settings::Entity);
    
    // Convert to SQL
    let settings_sql = match backend {
        DbBackend::Sqlite => create_settings_stmt.if_not_exists().to_string(SqliteQueryBuilder),
        DbBackend::Postgres => create_settings_stmt.if_not_exists().to_string(PostgresQueryBuilder),
        DbBackend::MySql => create_settings_stmt.if_not_exists().to_string(MysqlQueryBuilder),
    };
    
    // Execute the statement
    db.execute(Statement::from_string(backend, settings_sql)).await?;
    
    tracing::info!("Database migration completed");
    Ok(())
}