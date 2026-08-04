//! Configuration types for SDKWork connection pool management.
//!
//! This crate provides standardized configuration for database connection pools,
//! supporting both standalone and integrated deployment modes.
//!
//! Resolution is role-based (ENVIRONMENT_SPEC §7.2): [`DatabaseConfig::from_env`]
//! resolves the workspace PostgreSQL profile for authoritative-server modules,
//! while [`DatabaseConfig::load_client_local_from_env`] resolves
//! `SDKWORK_DATABASE_SQLITE_URL` for declared client-local data. Both roles
//! coexist in one process.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use sdkwork_database_config::{DatabaseConfig, DatabaseEngine, DeploymentMode};
//!
//! // Load the server profile config from environment variables
//! let config = DatabaseConfig::from_env("MY_SERVICE").unwrap();
//! ```

pub mod config_dir;
pub mod database;
pub mod env;
pub mod error;
pub mod postgres;
pub mod sqlite;
pub mod toml_config;
pub mod workspace_database;

pub use env::client_local_sqlite_url_configured;

// Re-export main types at crate root
pub use database::{DatabaseConfig, DatabaseEngine, DatabaseRole, DeploymentMode};
pub use error::ConfigError;
pub use postgres::{PgSslMode, PostgresConfig};
pub use sqlite::{SqliteConfig, SqliteJournalMode, SqliteSynchronous, SqliteTempStore};
