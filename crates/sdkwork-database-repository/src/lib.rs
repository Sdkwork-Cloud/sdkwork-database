//! Unified repository pattern abstraction for SDKWork database operations.
//!
//! This crate provides a unified interface for database operations that abstracts
//! away the differences between SQLite and PostgreSQL.
//!
//! # Modules
//!
//! - `entity`: Entity trait and macros for defining database entities
//! - `repository`: Repository trait and macros for CRUD operations
//! - `query`: Query builder for constructing SQL queries
//! - `types`: Common types like Pagination, SoftDelete, Versioned, etc.
//! - `advanced`: Batch operations and transaction management
//! - `migration`: Legacy inline migration macro (deprecated; use lifecycle framework)
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use sdkwork_database_repository::prelude::*;
//! use serde::{Deserialize, Serialize};
//!
//! // Define your entity
//! #[derive(Debug, Clone, Serialize, Deserialize)]
//! struct User {
//!     id: i64,
//!     name: String,
//!     email: String,
//! }
//!
//! // Implement Entity trait
//! impl_entity!(User, "users", id, [id, name, email]);
//!
//! // Create Repository
//! impl_repository!(User);
//! ```

pub mod advanced;
pub mod entity;
pub mod error;
pub mod health;
pub mod migration;
pub mod query;
pub mod repository;
pub mod types;

// Re-export main types at crate root
pub use entity::{AutoIdEntity, Entity, StringIdEntity};
pub use error::RepositoryError;
pub use health::{HealthChecker, HealthStatus};
pub use migration::{Migration, MigrationStatus};
pub use query::Query;
pub use repository::Repository;

// Re-export IdGenerator from sdkwork-id-core for use in macros
pub use sdkwork_id_core::IdGenerator;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::advanced::{BatchOperations, TransactionOperations};
    pub use crate::entity::{AutoIdEntity, Entity, StringIdEntity};
    pub use crate::error::RepositoryError;
    pub use crate::health::{HealthChecker, HealthStatus};
    pub use crate::migration::{Migration, MigrationStatus};
    pub use crate::query::Query;
    pub use crate::repository::Repository;
    pub use crate::IdGenerator;
    pub use sdkwork_id_core::{SnowflakeIdGenerator, UuidIdGenerator};
    pub use serde::{Deserialize, Serialize};
}
