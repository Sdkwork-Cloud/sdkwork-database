use std::fmt;

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine, DeploymentMode};
use sqlx::AnyPool;

use crate::any::create_any_pool;
use crate::error::PoolError;

/// Context information for a database pool.
#[derive(Debug, Clone)]
pub struct PoolContext {
    /// Original configuration.
    pub config: DatabaseConfig,
}

impl PoolContext {
    /// Get the deployment mode.
    pub fn mode(&self) -> DeploymentMode {
        self.config.mode
    }

    /// Get the table prefix.
    pub fn table_prefix(&self) -> &str {
        &self.config.table_prefix
    }

    /// Get the effective table name with prefix if in integrated mode.
    pub fn table_name(&self, name: &str) -> String {
        self.config.table_name(name)
    }
}

/// Unified database pool enum.
///
/// This enum wraps either a SQLite or PostgreSQL connection pool,
/// providing a unified interface for pool operations.
#[derive(Clone)]
pub enum DatabasePool {
    /// SQLite connection pool.
    Sqlite(sqlx::SqlitePool, PoolContext),

    /// PostgreSQL connection pool.
    Postgres(sqlx::PgPool, PoolContext),
}

impl fmt::Debug for DatabasePool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(_, ctx) => f.debug_tuple("Sqlite").field(&ctx.config.url).finish(),
            Self::Postgres(_, ctx) => f.debug_tuple("Postgres").field(&ctx.config.url).finish(),
        }
    }
}

impl DatabasePool {
    /// Get the pool context.
    pub fn context(&self) -> &PoolContext {
        match self {
            Self::Sqlite(_, ctx) => ctx,
            Self::Postgres(_, ctx) => ctx,
        }
    }

    /// Get the database configuration.
    pub fn config(&self) -> &DatabaseConfig {
        &self.context().config
    }

    /// Get the deployment mode.
    pub fn mode(&self) -> DeploymentMode {
        self.context().mode()
    }

    /// Get the table prefix.
    pub fn table_prefix(&self) -> &str {
        self.context().table_prefix()
    }

    /// Get the effective table name with prefix if in integrated mode.
    pub fn table_name(&self, name: &str) -> String {
        self.context().table_name(name)
    }

    /// Get the database engine type.
    pub fn engine(&self) -> DatabaseEngine {
        match self {
            Self::Sqlite(_, _) => DatabaseEngine::Sqlite,
            Self::Postgres(_, _) => DatabaseEngine::Postgres,
        }
    }

    /// Get the underlying SQLite pool, if this is a SQLite pool.
    pub fn as_sqlite(&self) -> Option<&sqlx::SqlitePool> {
        match self {
            Self::Sqlite(pool, _) => Some(pool),
            _ => None,
        }
    }

    /// Get the underlying PostgreSQL pool, if this is a PostgreSQL pool.
    pub fn as_postgres(&self) -> Option<&sqlx::PgPool> {
        match self {
            Self::Postgres(pool, _) => Some(pool),
            _ => None,
        }
    }

    /// Close the pool and all connections.
    pub async fn close(&self) {
        match self {
            Self::Sqlite(pool, _) => pool.close().await,
            Self::Postgres(pool, _) => pool.close().await,
        }
    }

    /// Get the number of idle connections.
    pub fn num_idle(&self) -> usize {
        match self {
            Self::Sqlite(pool, _) => pool.num_idle(),
            Self::Postgres(pool, _) => pool.num_idle(),
        }
    }

    /// Get the total size of the pool.
    pub fn size(&self) -> u32 {
        match self {
            Self::Sqlite(pool, _) => pool.size(),
            Self::Postgres(pool, _) => pool.size(),
        }
    }
}

/// Create a database pool from environment variables.
///
/// This is the recommended way to create a pool. It reads configuration
/// from environment variables following the SDKWORK naming convention.
///
/// # Environment Variables
///
/// - `SDKWORK_{SERVICE}_DATABASE_URL` - database connection URL
/// - `SDKWORK_{SERVICE}_DATABASE_MAX_CONNECTIONS` - max connections
/// - etc.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_sqlx::create_pool_from_env;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = create_pool_from_env("MY_SERVICE").await?;
///     println!("Pool created: {:?}", pool);
///     Ok(())
/// }
/// ```
pub async fn create_pool_from_env(service_name: &str) -> Result<Option<DatabasePool>, PoolError> {
    let config = match DatabaseConfig::from_env(service_name) {
        Ok(config) => config,
        Err(sdkwork_database_config::ConfigError::MissingRequired(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let pool = crate::builder::PoolBuilder::new(config).build().await?;
    Ok(Some(pool))
}

/// Create a database pool from a configuration.
pub async fn create_pool_from_config(config: DatabaseConfig) -> Result<DatabasePool, PoolError> {
    crate::builder::PoolBuilder::new(config).build().await
}

/// Create a sqlx AnyPool from a configuration.
pub async fn create_any_pool_from_config(config: DatabaseConfig) -> Result<AnyPool, PoolError> {
    let config = normalize_config_engine(config)?;
    create_any_pool(&config).await
}

/// Create a sqlx AnyPool from environment variables.
pub async fn create_any_pool_from_env(service_name: &str) -> Result<Option<AnyPool>, PoolError> {
    let config = match DatabaseConfig::from_env(service_name) {
        Ok(config) => config,
        Err(sdkwork_database_config::ConfigError::MissingRequired(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    Ok(Some(create_any_pool_from_config(config).await?))
}

fn normalize_config_engine(mut config: DatabaseConfig) -> Result<DatabaseConfig, PoolError> {
    if config.url.is_empty() {
        return Err(PoolError::InvalidUrl(
            "Database URL is empty. Set SDKWORK_*_DATABASE_URL environment variable.".to_string(),
        ));
    }

    if let Some(engine) = DatabaseEngine::from_url(&config.url) {
        config.engine = engine;
    }

    Ok(config)
}

/// Create a database pool from a TOML configuration file.
pub async fn create_pool_from_toml(path: &std::path::Path) -> Result<DatabasePool, PoolError> {
    let config = DatabaseConfig::from_toml_file(path)?;
    create_pool_from_config(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_any_pool_from_config() {
        let config = DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };

        let pool = create_any_pool_from_config(config).await.unwrap();
        sqlx::query("CREATE TABLE probe (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO probe (id, value) VALUES ($1, $2)")
            .bind(1_i64)
            .bind("ok")
            .execute(&pool)
            .await
            .unwrap();
        let value: String = sqlx::query_scalar("SELECT value FROM probe WHERE id=$1")
            .bind(1_i64)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(value, "ok");
        pool.close().await;
    }

    #[test]
    fn test_pool_context_table_name() {
        let config = DatabaseConfig {
            mode: DeploymentMode::Integrated,
            table_prefix: "forum_".to_string(),
            ..Default::default()
        };
        let ctx = PoolContext { config };

        assert_eq!(ctx.table_name("users"), "forum_users");
    }
}
