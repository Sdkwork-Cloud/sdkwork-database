use thiserror::Error;

/// Pool error types.
#[derive(Debug, Error)]
pub enum PoolError {
    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(#[from] sdkwork_database_config::ConfigError),

    /// Pool creation error (connection failed, timeout, etc.).
    #[error("Pool creation error: {0}")]
    PoolCreation(#[source] sqlx::Error),

    /// Invalid database URL.
    #[error("Invalid database URL: {0}")]
    InvalidUrl(String),

    /// Database connection error.
    #[error("Database connection error: {0}")]
    Connection(#[source] sqlx::Error),

    /// Database-specific configuration error.
    #[error("Database config error: {0}")]
    DatabaseConfig(String),

    /// Migration error.
    #[error("Migration error: {0}")]
    Migration(String),
}
