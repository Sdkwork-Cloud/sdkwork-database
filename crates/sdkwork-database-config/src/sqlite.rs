use serde::{Deserialize, Serialize};

/// SQLite journal mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SqliteJournalMode {
    Delete,
    Truncate,
    Persist,
    Memory,
    #[default]
    Wal,
    Off,
}

/// SQLite synchronous mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SqliteSynchronous {
    Off,
    #[default]
    Normal,
    Full,
    Extra,
}

/// SQLite temp store location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SqliteTempStore {
    Default,
    File,
    #[default]
    Memory,
}

/// SQLite-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    /// Journal mode for write-ahead logging.
    #[serde(default)]
    pub journal_mode: SqliteJournalMode,

    /// Busy timeout in seconds.
    #[serde(default = "default_busy_timeout")]
    pub busy_timeout_secs: u64,

    /// Enable foreign key constraints.
    #[serde(default = "default_true")]
    pub foreign_keys: bool,

    /// Synchronous mode.
    #[serde(default)]
    pub synchronous: SqliteSynchronous,

    /// Cache size in KB (negative value means KB, positive means pages).
    #[serde(default = "default_cache_size")]
    pub cache_size_kb: i64,

    /// Temp store location.
    #[serde(default)]
    pub temp_store: SqliteTempStore,

    /// Memory-mapped I/O size in bytes.
    #[serde(default = "default_mmap_size")]
    pub mmap_size_bytes: i64,

    /// Create the database file if it doesn't exist.
    #[serde(default = "default_true")]
    pub create_if_missing: bool,
}

fn default_busy_timeout() -> u64 {
    5
}

fn default_true() -> bool {
    true
}

fn default_cache_size() -> i64 {
    -64000 // 64MB in KB
}

fn default_mmap_size() -> i64 {
    268435456 // 256MB
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            journal_mode: SqliteJournalMode::default(),
            busy_timeout_secs: default_busy_timeout(),
            foreign_keys: default_true(),
            synchronous: SqliteSynchronous::default(),
            cache_size_kb: default_cache_size(),
            temp_store: SqliteTempStore::default(),
            mmap_size_bytes: default_mmap_size(),
            create_if_missing: default_true(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SqliteConfig::default();
        assert_eq!(config.journal_mode, SqliteJournalMode::Wal);
        assert_eq!(config.busy_timeout_secs, 5);
        assert!(config.foreign_keys);
        assert_eq!(config.synchronous, SqliteSynchronous::Normal);
        assert_eq!(config.cache_size_kb, -64000);
        assert_eq!(config.temp_store, SqliteTempStore::Memory);
        assert_eq!(config.mmap_size_bytes, 268435456);
        assert!(config.create_if_missing);
    }

    #[test]
    fn test_serialization() {
        let config = SqliteConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SqliteConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.journal_mode, deserialized.journal_mode);
        assert_eq!(config.busy_timeout_secs, deserialized.busy_timeout_secs);
    }
}
