use thiserror::Error;

/// Repository error types.
#[derive(Debug, Error)]
pub enum RepositoryError {
    /// Database error.
    #[error("Database error: {0}")]
    Database(#[source] sqlx::Error),

    /// Entity not found.
    #[error("Entity not found: {0}")]
    NotFound(String),

    /// Validation error.
    #[error("Validation error: {0}")]
    Validation(String),

    /// Query validation error (SQL injection prevention).
    #[error("Query validation error: {0}")]
    QueryValidation(String),

    /// Invalid column name (SQL injection prevention).
    #[error("Invalid column name: {0}")]
    InvalidColumnName(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(#[from] sdkwork_database_config::ConfigError),

    /// Pool error.
    #[error("Pool error: {0}")]
    Pool(#[from] sdkwork_database_sqlx::PoolError),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Migration error.
    #[error("Migration error: {0}")]
    Migration(String),

    /// ID generation error.
    #[error("ID generation error: {0}")]
    IdGeneration(String),

    /// Generic error.
    #[error("{0}")]
    Generic(String),
}

impl From<sqlx::Error> for RepositoryError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}
