//! Database-backed Snowflake ID generation for all SDKWork services.
//!
//! This crate is the **canonical** home for Snowflake-style int64 ID generation
//! across the SDKWork ecosystem. It provides:
//!
//! - **Re-exports** of the base [`SnowflakeIdGenerator`] from `sdkwork-id-core`
//!   so consumers only need a single dependency.
//! - **Database-backed node_id allocation** via [`SnowflakeNodeAllocator`],
//!   which eliminates manual node_id configuration and prevents collisions in
//!   auto-scaling environments (Kubernetes, etc.).
//!
//! # Quick Start (database-backed, recommended for production)
//!
//! ```rust,no_run
//! # use sdkwork_database_id::SnowflakeNodeAllocator;
//! # async fn example() -> Result<(), sdkwork_database_id::NodeAllocatorError> {
//! let (generator, _lease) =
//!     SnowflakeNodeAllocator::allocate_generator_from_env("my-service", "MEMORY").await?;
//! let id = generator.generate().map_err(|e| sdkwork_database_id::NodeAllocatorError::Snowflake(e))?;
//! # Ok(())
//! # }
//! ```
//!
//! # Fallback (env-based, for dev/test)
//!
//! ```rust
//! use sdkwork_database_id::SnowflakeIdGenerator;
//!
//! let generator = SnowflakeIdGenerator::new(0).unwrap();
//! let id = generator.generate().unwrap();
//! ```

// Re-export everything from sdkwork-id-core so consumers only need this crate.
pub use sdkwork_id_core::{
    current_time_millis, default_snowflake_epoch_millis, default_snowflake_profile, generate_batch,
    max_snowflake_node_id, uuid_v4, uuid_v4_with_prefix, validate_snowflake_id, IdGenError,
    IdGenerator, SnowflakeIdError, SnowflakeIdGenerator, SnowflakeProfile, UuidIdGenerator,
};

mod node_allocator;

pub use node_allocator::{
    NodeAllocatorConfig, NodeAllocatorError, NodeLease, SnowflakeNodeAllocator,
};
