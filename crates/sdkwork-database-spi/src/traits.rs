use std::path::PathBuf;

use async_trait::async_trait;
use sdkwork_database_config::DatabaseEngine;

use crate::error::SpiError;
use crate::types::{
    DatabaseModuleDescriptor, DriftPolicy, LifecycleFailureEvent, LifecycleStateEvent, LocaleTag,
    MigrationContext, MigrationSpec, SeedContext, SeedPlan, SeedProfile,
};

#[async_trait]
pub trait DatabaseAssetProvider: Send + Sync {
    fn module_root(&self) -> &std::path::Path;
    fn manifest_path(&self) -> PathBuf;
    fn contract_path(&self) -> PathBuf;
    fn migrations_dir(&self, engine: DatabaseEngine) -> PathBuf;
    fn seeds_dir(&self) -> PathBuf;
    fn drift_policy_path(&self) -> PathBuf;

    fn baseline_dir(&self, engine: DatabaseEngine) -> PathBuf {
        let engine_dir = match engine {
            DatabaseEngine::Postgres => "postgres",
            DatabaseEngine::Sqlite => "sqlite",
        };
        self.module_root().join("ddl/baseline").join(engine_dir)
    }
}

#[async_trait]
pub trait DatabaseContractProvider: Send + Sync {
    async fn contract_version(&self) -> Result<String, SpiError>;
}

#[async_trait]
pub trait MigrationProvider: Send + Sync {
    async fn list_migrations(&self, engine: DatabaseEngine)
        -> Result<Vec<MigrationSpec>, SpiError>;

    async fn before_migration(&self, _ctx: &MigrationContext) -> Result<(), SpiError> {
        Ok(())
    }

    async fn after_migration(&self, _ctx: &MigrationContext) -> Result<(), SpiError> {
        Ok(())
    }
}

#[async_trait]
pub trait SeedProvider: Send + Sync {
    async fn resolve_seed_plan(
        &self,
        locale: &LocaleTag,
        profile: &SeedProfile,
    ) -> Result<SeedPlan, SpiError>;

    async fn before_seed(&self, _ctx: &SeedContext) -> Result<(), SpiError> {
        Ok(())
    }

    async fn after_seed(&self, _ctx: &SeedContext) -> Result<(), SpiError> {
        Ok(())
    }
}

#[async_trait]
pub trait DriftPolicyProvider: Send + Sync {
    async fn load_policy(&self) -> Result<DriftPolicy, SpiError>;
}

#[async_trait]
pub trait SchemaIntrospector: Send + Sync {
    async fn introspect(&self) -> Result<String, SpiError>;
}

#[async_trait]
pub trait DatabaseLifecycleListener: Send + Sync {
    async fn on_state_change(&self, _event: LifecycleStateEvent) -> Result<(), SpiError> {
        Ok(())
    }

    async fn on_failure(&self, _event: LifecycleFailureEvent) -> Result<(), SpiError> {
        Ok(())
    }
}

pub trait DatabaseModuleDescriptorProvider: Send + Sync {
    fn descriptor(&self) -> DatabaseModuleDescriptor;
}

#[async_trait]
pub trait DatabaseModule:
    DatabaseModuleDescriptorProvider
    + DatabaseAssetProvider
    + DatabaseContractProvider
    + MigrationProvider
    + SeedProvider
    + DriftPolicyProvider
{
    fn listeners(&self) -> Vec<Box<dyn DatabaseLifecycleListener>> {
        Vec::new()
    }
}
