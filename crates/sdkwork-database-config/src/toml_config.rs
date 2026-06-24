use std::fs;
use std::path::Path;

use crate::database::DatabaseConfig;
use crate::error::ConfigError;

/// Load database configuration from a TOML file.
///
/// # File Format
///
/// ```toml
/// [database]
/// engine = "sqlite"
/// url = "sqlite:data.db"
/// mode = "standalone"
/// max_connections = 8
/// min_connections = 1
/// acquire_timeout_secs = 10
/// idle_timeout_secs = 300
/// max_lifetime_secs = 1800
///
/// [database.sqlite]
/// journal_mode = "wal"
/// busy_timeout_secs = 5
/// foreign_keys = true
/// synchronous = "normal"
/// cache_size_kb = 64000
/// temp_store = "memory"
/// mmap_size_bytes = 268435456
///
/// [database.postgres]
/// statement_cache_capacity = 100
/// application_name = "sdkwork-app"
/// ```
pub fn load_from_toml(path: &Path) -> Result<DatabaseConfig, ConfigError> {
    let content = fs::read_to_string(path)?;
    let config: TomlConfig = toml::from_str(&content)?;
    Ok(config.database)
}

/// TOML file structure.
#[derive(Debug, serde::Deserialize)]
struct TomlConfig {
    database: DatabaseConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_from_toml_sqlite() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[database]
engine = "sqlite"
url = "sqlite:test.db"
max_connections = 8

[database.sqlite]
journal_mode = "wal"
busy_timeout_secs = 10
"#
        )
        .unwrap();

        let config = load_from_toml(file.path()).unwrap();
        assert_eq!(config.engine, crate::database::DatabaseEngine::Sqlite);
        assert_eq!(config.url, "sqlite:test.db");
        assert_eq!(config.max_connections, 8);
        assert_eq!(
            config.sqlite.journal_mode,
            crate::sqlite::SqliteJournalMode::Wal
        );
        assert_eq!(config.sqlite.busy_timeout_secs, 10);
    }

    #[test]
    fn test_load_from_toml_postgres() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[database]
engine = "postgres"
url = "postgres://localhost/test"
max_connections = 16

[database.postgres]
statement_cache_capacity = 200
application_name = "my-app"
"#
        )
        .unwrap();

        let config = load_from_toml(file.path()).unwrap();
        assert_eq!(config.engine, crate::database::DatabaseEngine::Postgres);
        assert_eq!(config.max_connections, 16);
        assert_eq!(config.postgres.statement_cache_capacity, 200);
        assert_eq!(config.postgres.application_name, Some("my-app".to_string()));
    }

    #[test]
    fn test_load_from_toml_integrated_mode() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[database]
engine = "postgres"
url = "postgres://localhost/shared"
mode = "integrated"
table_prefix = "forum_"
"#
        )
        .unwrap();

        let config = load_from_toml(file.path()).unwrap();
        assert_eq!(config.mode, crate::database::DeploymentMode::Integrated);
        assert_eq!(config.table_prefix, "forum_");
    }
}
