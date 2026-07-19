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

    /// A module requested a different database identity after the process pool was installed.
    #[error(
        "Process-shared database pool identity mismatch: installed [{installed}], requested [{requested}]"
    )]
    ProcessPoolIdentityMismatch {
        /// Redacted identity of the installed process pool.
        installed: String,
        /// Redacted identity requested by the later consumer.
        requested: String,
    },

    /// A module requested a pool driver that cannot reuse the installed process pool.
    #[error(
        "Process-shared database pool driver mismatch: installed driver [{installed}], requested driver [{requested}]"
    )]
    ProcessPoolDriverMismatch {
        /// Driver owned by the process pool.
        installed: &'static str,
        /// Driver requested by the consumer.
        requested: &'static str,
    },

    /// A compatibility driver was requested before the canonical process pool existed.
    #[error("Process-shared database pool is not installed before compatibility driver request")]
    ProcessPoolNotInstalled,

    /// The temporary driver flag was enabled only after the canonical pool was created.
    #[error(
        "Temporary database driver capacity was not reserved before canonical process pool creation"
    )]
    TemporaryDriverCapacityNotReserved,
}

impl From<sqlx::Error> for PoolError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => Self::PoolCreation(err),
            _ => Self::Connection(err),
        }
    }
}
