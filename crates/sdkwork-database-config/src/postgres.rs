use serde::{Deserialize, Serialize};

/// PostgreSQL SSL mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PgSslMode {
    /// Only try non-SSL connections.
    Disable,
    /// First try non-SSL, then SSL.
    Allow,
    /// First try SSL, then non-SSL.
    #[default]
    Prefer,
    /// Only try SSL connections.
    Require,
    /// Only try SSL with CA verification.
    VerifyCa,
    /// Only try SSL with full verification.
    VerifyFull,
}

/// PostgreSQL-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    /// Statement cache capacity.
    #[serde(default = "default_statement_cache_capacity")]
    pub statement_cache_capacity: usize,

    /// Application name for pg_stat_activity.
    #[serde(default = "default_application_name")]
    pub application_name: Option<String>,

    /// SSL mode for PostgreSQL connections.
    #[serde(default)]
    pub ssl_mode: PgSslMode,

    /// Path to root CA certificate file.
    #[serde(default)]
    pub ssl_root_cert: Option<String>,

    /// Path to client certificate file.
    #[serde(default)]
    pub ssl_client_cert: Option<String>,

    /// Path to client key file.
    #[serde(default)]
    pub ssl_client_key: Option<String>,
}

fn default_statement_cache_capacity() -> usize {
    100
}

fn default_application_name() -> Option<String> {
    Some("sdkwork".to_string())
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            statement_cache_capacity: default_statement_cache_capacity(),
            application_name: default_application_name(),
            ssl_mode: PgSslMode::default(),
            ssl_root_cert: None,
            ssl_client_cert: None,
            ssl_client_key: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PostgresConfig::default();
        assert_eq!(config.statement_cache_capacity, 100);
        assert_eq!(config.application_name, Some("sdkwork".to_string()));
        assert_eq!(config.ssl_mode, PgSslMode::Prefer);
        assert!(config.ssl_root_cert.is_none());
    }

    #[test]
    fn test_serialization() {
        let config = PostgresConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: PostgresConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            config.statement_cache_capacity,
            deserialized.statement_cache_capacity
        );
        assert_eq!(config.ssl_mode, deserialized.ssl_mode);
    }
}
