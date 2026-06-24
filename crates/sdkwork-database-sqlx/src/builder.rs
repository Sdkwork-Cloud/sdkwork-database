use std::time::Duration;

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine, DeploymentMode};

use crate::error::PoolError;
use crate::pool::DatabasePool;
use crate::postgres::create_postgres_pool;
use crate::sqlite::create_sqlite_pool;

/// Builder for creating database connection pools.
///
/// # Example
///
/// ```rust,no_run
/// use std::time::Duration;
/// use sdkwork_database_config::DatabaseConfig;
/// use sdkwork_database_sqlx::PoolBuilder;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = DatabaseConfig::from_env("MY_SERVICE")?;
///     let pool = PoolBuilder::new(config)
///         .max_connections(32)
///         .acquire_timeout(Duration::from_secs(30))
///         .build()
///         .await?;
///     Ok(())
/// }
/// ```
pub struct PoolBuilder {
    config: DatabaseConfig,
}

impl PoolBuilder {
    /// Create a new pool builder with the given configuration.
    pub fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }

    /// Create a builder from environment variables for a given service.
    pub fn from_env(service_name: &str) -> Result<Self, sdkwork_database_config::ConfigError> {
        let config = DatabaseConfig::from_env(service_name)?;
        Ok(Self { config })
    }

    /// Create a builder from a TOML configuration file.
    pub fn from_toml(path: &std::path::Path) -> Result<Self, sdkwork_database_config::ConfigError> {
        let config = DatabaseConfig::from_toml_file(path)?;
        Ok(Self { config })
    }

    /// Set the database engine type.
    pub fn engine(mut self, engine: DatabaseEngine) -> Self {
        self.config.engine = engine;
        self
    }

    /// Set the database URL.
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.config.url = url.into();
        self
    }

    /// Set the deployment mode.
    pub fn mode(mut self, mode: DeploymentMode) -> Self {
        self.config.mode = mode;
        self
    }

    /// Set the table prefix for integrated mode.
    pub fn table_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.config.table_prefix = prefix.into();
        self
    }

    /// Set the maximum number of connections.
    pub fn max_connections(mut self, max: u32) -> Self {
        self.config.max_connections = max;
        self
    }

    /// Set the minimum number of idle connections.
    pub fn min_connections(mut self, min: u32) -> Self {
        self.config.min_connections = min;
        self
    }

    /// Set the connection acquisition timeout.
    pub fn acquire_timeout(mut self, timeout: Duration) -> Self {
        self.config.acquire_timeout_secs = timeout.as_secs();
        self
    }

    /// Set the idle connection timeout.
    pub fn idle_timeout(mut self, timeout: Duration) -> Self {
        self.config.idle_timeout_secs = timeout.as_secs();
        self
    }

    /// Set the maximum connection lifetime.
    pub fn max_lifetime(mut self, lifetime: Duration) -> Self {
        self.config.max_lifetime_secs = lifetime.as_secs();
        self
    }

    /// Build the connection pool.
    pub async fn build(self) -> Result<DatabasePool, PoolError> {
        // Validate configuration
        if self.config.url.is_empty() {
            return Err(PoolError::InvalidUrl(
                "Database URL is empty. Set SDKWORK_*_DATABASE_URL environment variable."
                    .to_string(),
            ));
        }

        // Auto-detect engine if not set
        let config = if self.config.engine == DatabaseEngine::Sqlite
            && self.config.url.starts_with("postgres")
        {
            let mut config = self.config;
            config.engine = DatabaseEngine::Postgres;
            config
        } else if self.config.engine == DatabaseEngine::Postgres
            && self.config.url.starts_with("sqlite")
        {
            let mut config = self.config;
            config.engine = DatabaseEngine::Sqlite;
            config
        } else {
            self.config
        };

        match config.engine {
            DatabaseEngine::Sqlite => {
                let (pool, ctx) = create_sqlite_pool(&config).await?;
                Ok(DatabasePool::Sqlite(pool, ctx))
            }
            DatabaseEngine::Postgres => {
                let (pool, ctx) = create_postgres_pool(&config).await?;
                Ok(DatabasePool::Postgres(pool, ctx))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_database_config::{DatabaseEngine, DeploymentMode};

    #[tokio::test]
    async fn test_build_sqlite_pool() {
        let config = DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            mode: DeploymentMode::Standalone,
            max_connections: 1,
            ..Default::default()
        };

        let pool = PoolBuilder::new(config).build().await.unwrap();
        assert_eq!(pool.engine(), DatabaseEngine::Sqlite);
        assert_eq!(pool.mode(), DeploymentMode::Standalone);

        pool.close().await;
    }

    #[tokio::test]
    async fn test_build_sqlite_pool_integrated() {
        let config = DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            mode: DeploymentMode::Integrated,
            table_prefix: "forum_".to_string(),
            max_connections: 1,
            ..Default::default()
        };

        let pool = PoolBuilder::new(config).build().await.unwrap();
        assert_eq!(pool.mode(), DeploymentMode::Integrated);
        assert_eq!(pool.table_name("users"), "forum_users");

        pool.close().await;
    }

    #[tokio::test]
    async fn test_builder_chaining() {
        let config = DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            ..Default::default()
        };

        let pool = PoolBuilder::new(config)
            .max_connections(4)
            .mode(DeploymentMode::Integrated)
            .table_prefix("test_")
            .build()
            .await
            .unwrap();

        assert_eq!(pool.mode(), DeploymentMode::Integrated);
        assert_eq!(pool.table_name("users"), "test_users");
        assert_eq!(pool.config().max_connections, 4);

        pool.close().await;
    }

    #[tokio::test]
    async fn test_builder_empty_url_error() {
        let config = DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: String::new(),
            ..Default::default()
        };

        let result = PoolBuilder::new(config).build().await;
        assert!(result.is_err());
    }
}
