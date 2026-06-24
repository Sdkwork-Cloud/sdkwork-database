//! Database lifecycle SPI for SDKWork applications.
//!
//! Applications implement these traits or use [`DefaultDatabaseModule`] with
//! standard `database/` assets. The lifecycle orchestrator in
//! `sdkwork-database-lifecycle` consumes registered modules.

pub mod drift_policy;
pub mod error;
pub mod layout;
pub mod manifest;
pub mod module;
pub mod registry;
pub mod seed_manifest;
pub mod traits;
pub mod types;

pub use drift_policy::DriftPolicyFile;
pub use error::SpiError;
pub use layout::validate_module_layout;
pub use manifest::DatabaseManifest;
pub use module::DefaultDatabaseModule;
pub use registry::DatabaseModuleRegistry;
pub use seed_manifest::{SeedManifest, SeedProfileDefinition};
pub use traits::{
    DatabaseAssetProvider, DatabaseContractProvider, DatabaseLifecycleListener, DatabaseModule,
    DriftPolicyProvider, MigrationProvider, SchemaIntrospector, SeedProvider,
};
pub use types::{
    DatabaseModuleDescriptor, DriftPolicy, LifecycleFailureEvent, LifecycleOptions, LifecycleState,
    LifecycleStateEvent, LocaleTag, MigrationContext, MigrationSpec, SeedContext, SeedPlan,
    SeedProfile,
};
