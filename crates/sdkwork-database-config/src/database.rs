use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

use crate::postgres::PostgresConfig;
use crate::sqlite::SqliteConfig;

/// Database engine type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseEngine {
    Sqlite,
    Postgres,
}

impl fmt::Display for DatabaseEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseEngine::Sqlite => write!(f, "sqlite"),
            DatabaseEngine::Postgres => write!(f, "postgres"),
        }
    }
}

impl DatabaseEngine {
    /// Detect engine from URL scheme.
    pub fn from_url(url: &str) -> Option<Self> {
        if url.starts_with("sqlite:") || url.starts_with("sqlite::") {
            Some(Self::Sqlite)
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Some(Self::Postgres)
        } else {
            None
        }
    }
}

/// Deployment mode for database connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    /// Each service has its own database file/instance.
    #[default]
    Standalone,
    /// All services share one database with table prefixes.
    Integrated,
}

/// Main database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database engine type.
    pub engine: DatabaseEngine,

    /// Database connection URL.
    pub url: String,

    /// Deployment mode (standalone or integrated).
    #[serde(default)]
    pub mode: DeploymentMode,

    /// Table prefix for integrated mode.
    #[serde(default)]
    pub table_prefix: String,

    /// Maximum number of connections in the pool.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// Minimum number of idle connections.
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,

    /// Timeout for acquiring a connection (seconds).
    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout_secs: u64,

    /// Timeout for idle connections (seconds).
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,

    /// Maximum lifetime of a connection (seconds).
    #[serde(default = "default_max_lifetime")]
    pub max_lifetime_secs: u64,

    /// SQLite-specific configuration.
    #[serde(default)]
    pub sqlite: SqliteConfig,

    /// PostgreSQL-specific configuration.
    #[serde(default)]
    pub postgres: PostgresConfig,
}

fn default_max_connections() -> u32 {
    16
}

fn default_min_connections() -> u32 {
    1
}

fn default_acquire_timeout() -> u64 {
    10
}

fn default_idle_timeout() -> u64 {
    300
}

fn default_max_lifetime() -> u64 {
    1800
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            engine: DatabaseEngine::Sqlite,
            url: String::new(),
            mode: DeploymentMode::default(),
            table_prefix: String::new(),
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            acquire_timeout_secs: default_acquire_timeout(),
            idle_timeout_secs: default_idle_timeout(),
            max_lifetime_secs: default_max_lifetime(),
            sqlite: SqliteConfig::default(),
            postgres: PostgresConfig::default(),
        }
    }
}

impl DatabaseConfig {
    /// Get acquire timeout as Duration.
    pub fn acquire_timeout(&self) -> Duration {
        Duration::from_secs(self.acquire_timeout_secs)
    }

    /// Get idle timeout as Duration.
    pub fn idle_timeout(&self) -> Duration {
        Duration::from_secs(self.idle_timeout_secs)
    }

    /// Get max lifetime as Duration.
    pub fn max_lifetime(&self) -> Duration {
        Duration::from_secs(self.max_lifetime_secs)
    }

    /// Get the effective table name with prefix if in integrated mode.
    pub fn table_name(&self, name: &str) -> String {
        match self.mode {
            DeploymentMode::Standalone => name.to_string(),
            DeploymentMode::Integrated => {
                if self.table_prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{}{}", self.table_prefix, name)
                }
            }
        }
    }

    /// Load configuration from environment variables for a given service.
    pub fn from_env(service_name: &str) -> Result<Self, crate::ConfigError> {
        crate::env::load_from_env(service_name)
    }

    /// Load configuration from a TOML file.
    pub fn from_toml_file(path: &std::path::Path) -> Result<Self, crate::ConfigError> {
        crate::toml_config::load_from_toml(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_from_url() {
        assert_eq!(
            DatabaseEngine::from_url("sqlite:data.db"),
            Some(DatabaseEngine::Sqlite)
        );
        assert_eq!(
            DatabaseEngine::from_url("sqlite::memory:"),
            Some(DatabaseEngine::Sqlite)
        );
        assert_eq!(
            DatabaseEngine::from_url("postgres://localhost/db"),
            Some(DatabaseEngine::Postgres)
        );
        assert_eq!(
            DatabaseEngine::from_url("postgresql://localhost/db"),
            Some(DatabaseEngine::Postgres)
        );
        assert_eq!(DatabaseEngine::from_url("mysql://localhost/db"), None);
    }

    #[test]
    fn test_table_name_standalone() {
        let config = DatabaseConfig {
            mode: DeploymentMode::Standalone,
            ..Default::default()
        };
        assert_eq!(config.table_name("users"), "users");
    }

    #[test]
    fn test_table_name_integrated() {
        let config = DatabaseConfig {
            mode: DeploymentMode::Integrated,
            table_prefix: "forum_".to_string(),
            ..Default::default()
        };
        assert_eq!(config.table_name("users"), "forum_users");
    }

    #[test]
    fn test_table_name_integrated_no_prefix() {
        let config = DatabaseConfig {
            mode: DeploymentMode::Integrated,
            table_prefix: String::new(),
            ..Default::default()
        };
        assert_eq!(config.table_name("users"), "users");
    }
}
