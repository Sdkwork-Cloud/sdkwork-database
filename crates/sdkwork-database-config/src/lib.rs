//! Configuration types for SDKWork connection pool management.
//!
//! This crate provides standardized configuration for database connection pools,
//! supporting both standalone and integrated deployment modes.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use sdkwork_database_config::{DatabaseConfig, DatabaseEngine, DeploymentMode};
//!
//! // Load config from environment variables
//! let config = DatabaseConfig::from_env("MY_SERVICE").unwrap();
//! ```

pub mod claw_database;
pub mod database;
pub mod env;
pub mod error;
pub mod postgres;
pub mod sqlite;
pub mod toml_config;

// Re-export main types at crate root
pub use database::{DatabaseConfig, DatabaseEngine, DeploymentMode};
pub use error::ConfigError;
pub use postgres::{PgSslMode, PostgresConfig};
pub use sqlite::{SqliteConfig, SqliteJournalMode, SqliteSynchronous, SqliteTempStore};
