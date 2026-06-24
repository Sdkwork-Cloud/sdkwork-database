use thiserror::Error;

/// Configuration error types.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Environment variable not found.
    #[error("Environment variable not found: {0}")]
    EnvNotFound(String),

    /// Invalid environment variable value.
    #[error("Invalid environment variable {key}: {message}")]
    InvalidEnvValue { key: String, message: String },

    /// Invalid database URL.
    #[error("Invalid database URL: {0}")]
    InvalidUrl(String),

    /// TOML parsing error.
    #[error("TOML parsing error: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Missing required configuration.
    #[error("Missing required configuration: {0}")]
    MissingRequired(String),

    /// Invalid configuration value.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
