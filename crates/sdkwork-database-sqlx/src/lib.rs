//! Sqlx-based connection pool implementation for SDKWork.
//!
//! This crate provides a unified interface for creating and managing database
//! connection pools using sqlx. It supports both SQLite and PostgreSQL.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use sdkwork_database_sqlx::create_pool_from_env;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let pool = create_pool_from_env("MY_SERVICE").await?;
//!     println!("Pool created: {:?}", pool);
//!     Ok(())
//! }
//! ```
//!
//! # Using the Builder
//!
//! ```rust,no_run
//! use std::time::Duration;
//! use sdkwork_database_config::{DatabaseConfig, DeploymentMode};
//! use sdkwork_database_sqlx::PoolBuilder;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = DatabaseConfig::from_env("MY_SERVICE")?;
//!     let pool = PoolBuilder::new(config)
//!         .max_connections(32)
//!         .acquire_timeout(Duration::from_secs(30))
//!         .mode(DeploymentMode::Integrated)
//!         .table_prefix("my_service_")
//!         .build()
//!         .await?;
//!     Ok(())
//! }
//! ```

pub mod any;
pub mod builder;
pub mod error;
pub mod pool;
pub mod postgres;
pub mod sqlite;

// Re-export main types at crate root
pub use builder::PoolBuilder;
pub use error::PoolError;
pub use pool::{
    create_any_pool_from_config, create_any_pool_from_env, create_pool_from_config,
    create_pool_from_env, create_pool_from_toml, DatabasePool, PoolContext,
};
