use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use tracing::info;

use sdkwork_database_config::{DatabaseConfig, SqliteTempStore};

use crate::error::PoolError;
use crate::pool::PoolContext;
use crate::sqlite_decimal::register_decimal_functions;

/// Create a SQLite connection pool from configuration.
pub async fn create_sqlite_pool(
    config: &DatabaseConfig,
) -> Result<(SqlitePool, PoolContext), PoolError> {
    let sqlite_config = &config.sqlite;

    let connect_options = SqliteConnectOptions::from_str(&config.url)
        .map_err(|e| PoolError::InvalidUrl(format!("{}: {}", config.url, e)))?
        .create_if_missing(sqlite_config.create_if_missing)
        .journal_mode(map_journal_mode(sqlite_config.journal_mode))
        .busy_timeout(Duration::from_secs(sqlite_config.busy_timeout_secs))
        .foreign_keys(sqlite_config.foreign_keys)
        .synchronous(map_synchronous(sqlite_config.synchronous))
        .pragma("cache_size", sqlite_config.cache_size_kb.to_string())
        .pragma("temp_store", map_temp_store(sqlite_config.temp_store))
        .pragma("mmap_size", sqlite_config.mmap_size_bytes.to_string());

    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout())
        .idle_timeout(config.idle_timeout())
        .max_lifetime(config.max_lifetime())
        .after_connect(|connection, _metadata| {
            Box::pin(async move { register_decimal_functions(connection).await })
        })
        .connect_with(connect_options)
        .await
        .map_err(PoolError::PoolCreation)?;

    info!(
        engine = "sqlite",
        url = %mask_url(&config.url),
        max_connections = config.max_connections,
        journal_mode = ?sqlite_config.journal_mode,
        "SQLite connection pool created"
    );

    let ctx = PoolContext {
        config: config.clone(),
    };

    Ok((pool, ctx))
}

fn map_journal_mode(mode: sdkwork_database_config::SqliteJournalMode) -> SqliteJournalMode {
    match mode {
        sdkwork_database_config::SqliteJournalMode::Delete => SqliteJournalMode::Delete,
        sdkwork_database_config::SqliteJournalMode::Truncate => SqliteJournalMode::Truncate,
        sdkwork_database_config::SqliteJournalMode::Persist => SqliteJournalMode::Persist,
        sdkwork_database_config::SqliteJournalMode::Memory => SqliteJournalMode::Memory,
        sdkwork_database_config::SqliteJournalMode::Wal => SqliteJournalMode::Wal,
        sdkwork_database_config::SqliteJournalMode::Off => SqliteJournalMode::Off,
    }
}

fn map_synchronous(mode: sdkwork_database_config::SqliteSynchronous) -> SqliteSynchronous {
    match mode {
        sdkwork_database_config::SqliteSynchronous::Off => SqliteSynchronous::Off,
        sdkwork_database_config::SqliteSynchronous::Normal => SqliteSynchronous::Normal,
        sdkwork_database_config::SqliteSynchronous::Full => SqliteSynchronous::Full,
        sdkwork_database_config::SqliteSynchronous::Extra => SqliteSynchronous::Extra,
    }
}

fn map_temp_store(mode: SqliteTempStore) -> &'static str {
    match mode {
        SqliteTempStore::Default => "default",
        SqliteTempStore::File => "file",
        SqliteTempStore::Memory => "memory",
    }
}

/// Mask sensitive parts of a URL for logging.
fn mask_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(scheme_end) = url.find("://") {
            let scheme = &url[..scheme_end + 3];
            let host_and_rest = &url[at_pos..];
            return format!("{}***:***{}", scheme, host_and_rest);
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_database_config::{DatabaseEngine, DeploymentMode};

    #[tokio::test]
    async fn test_create_sqlite_pool_in_memory() {
        let config = DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            mode: DeploymentMode::Standalone,
            max_connections: 1,
            ..Default::default()
        };

        let (pool, ctx) = create_sqlite_pool(&config).await.unwrap();
        assert_eq!(ctx.mode(), DeploymentMode::Standalone);
        assert!(pool.size() <= 1);

        pool.close().await;
    }

    #[tokio::test]
    async fn test_create_sqlite_pool_with_prefix() {
        let config = DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            mode: DeploymentMode::Integrated,
            table_prefix: "test_".to_string(),
            max_connections: 1,
            ..Default::default()
        };

        let (pool, ctx) = create_sqlite_pool(&config).await.unwrap();
        assert_eq!(ctx.table_name("users"), "test_users");

        pool.close().await;
    }
}
