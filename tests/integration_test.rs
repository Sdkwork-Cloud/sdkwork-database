use sdkwork_database_config::{DatabaseConfig, DatabaseEngine, DeploymentMode};
use sdkwork_database_sqlx::{create_pool_from_config, PoolBuilder};

#[tokio::test]
async fn test_sqlite_standalone_mode() {
    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        mode: DeploymentMode::Standalone,
        max_connections: 2,
        ..Default::default()
    };
    
    let pool = create_pool_from_config(config).await.unwrap();
    
    assert_eq!(pool.engine(), DatabaseEngine::Sqlite);
    assert_eq!(pool.mode(), DeploymentMode::Standalone);
    assert_eq!(pool.table_name("users"), "users");
    
    pool.close().await;
}

#[tokio::test]
async fn test_sqlite_integrated_mode() {
    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        mode: DeploymentMode::Integrated,
        table_prefix: "forum_".to_string(),
        max_connections: 2,
        ..Default::default()
    };
    
    let pool = create_pool_from_config(config).await.unwrap();
    
    assert_eq!(pool.engine(), DatabaseEngine::Sqlite);
    assert_eq!(pool.mode(), DeploymentMode::Integrated);
    assert_eq!(pool.table_name("users"), "forum_users");
    assert_eq!(pool.table_name("threads"), "forum_threads");
    
    pool.close().await;
}

#[tokio::test]
async fn test_sqlite_custom_config() {
    let mut config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        mode: DeploymentMode::Standalone,
        max_connections: 4,
        ..Default::default()
    };
    
    // Customize SQLite config
    config.sqlite.busy_timeout_secs = 10;
    config.sqlite.foreign_keys = true;
    
    let pool = create_pool_from_config(config).await.unwrap();
    
    assert_eq!(pool.engine(), DatabaseEngine::Sqlite);
    
    pool.close().await;
}

#[tokio::test]
async fn test_builder_pattern() {
    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        mode: DeploymentMode::Standalone,
        max_connections: 1,
        ..Default::default()
    };
    
    let pool = PoolBuilder::new(config).build().await.unwrap();
    
    assert_eq!(pool.engine(), DatabaseEngine::Sqlite);
    
    pool.close().await;
}

#[tokio::test]
async fn test_pool_context() {
    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        mode: DeploymentMode::Integrated,
        table_prefix: "app_".to_string(),
        max_connections: 1,
        ..Default::default()
    };
    
    let pool = create_pool_from_config(config).await.unwrap();
    let ctx = pool.context();
    
    assert_eq!(ctx.mode, DeploymentMode::Integrated);
    assert_eq!(ctx.table_prefix, "app_");
    assert_eq!(ctx.table_name("settings"), "app_settings");
    
    pool.close().await;
}
