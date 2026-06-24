//! Legacy migration types retained for the `define_migrations!` macro only.
//!
//! Application database lifecycle MUST use `sdkwork-database-lifecycle`
//! (`LifecycleOrchestrator`, `sdkwork-db` CLI, and app `database/` module layout).

use serde::{Deserialize, Serialize};

/// Migration status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MigrationStatus {
    /// Migration has been applied.
    Applied,
    /// Migration is pending.
    Pending,
    /// Migration failed.
    Failed(String),
}

/// A single migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    /// Migration version (e.g., "001", "002", etc.).
    pub version: String,
    /// Migration name.
    pub name: String,
    /// SQL to apply the migration.
    pub up_sql: String,
    /// SQL to rollback the migration.
    pub down_sql: String,
    /// Migration status.
    pub status: MigrationStatus,
}

/// Macro to define migrations inline.
///
/// **Deprecated:** use app-root `database/migrations/` with `LifecycleOrchestrator`.
///
/// # Example
///
/// ```rust
/// use sdkwork_database_repository::define_migrations;
///
/// define_migrations!(
///     "001_create_users" => {
///         up: "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL);",
///         down: "DROP TABLE IF EXISTS users;"
///     }
/// );
///
/// let migrations = get_migrations();
/// assert_eq!(migrations.len(), 1);
/// ```
#[macro_export]
macro_rules! define_migrations {
    ($($name:expr => { up: $up:expr, down: $down:expr }),* $(,)?) => {
        pub fn get_migrations() -> Vec<$crate::migration::Migration> {
            vec![
                $(
                    $crate::migration::Migration {
                        version: $name.split('_').next().unwrap_or("0").to_string(),
                        name: $name.to_string(),
                        up_sql: $up.to_string(),
                        down_sql: $down.to_string(),
                        status: $crate::migration::MigrationStatus::Pending,
                    },
                )*
            ]
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_status() {
        assert_eq!(MigrationStatus::Applied, MigrationStatus::Applied);
        assert_ne!(MigrationStatus::Applied, MigrationStatus::Pending);
    }

    #[test]
    fn test_define_migrations_macro() {
        define_migrations!(
            "001_create_users" => {
                up: "CREATE TABLE users (id INTEGER PRIMARY KEY);",
                down: "DROP TABLE users;"
            },
            "002_add_email" => {
                up: "ALTER TABLE users ADD COLUMN email TEXT;",
                down: "ALTER TABLE users DROP COLUMN email;"
            }
        );

        let migrations = get_migrations();
        assert_eq!(migrations.len(), 2);
        assert_eq!(migrations[0].version, "001");
        assert_eq!(migrations[0].name, "001_create_users");
        assert_eq!(migrations[1].version, "002");
    }
}
