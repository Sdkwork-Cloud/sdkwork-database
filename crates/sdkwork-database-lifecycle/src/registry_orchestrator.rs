use sdkwork_database_spi::{DatabaseModuleRegistry, LocaleTag, SeedProfile};

use crate::error::LifecycleError;
use crate::orchestrator::LifecycleOrchestrator;

pub struct RegistryLifecycleOrchestrator {
    pool: sdkwork_database_sqlx::DatabasePool,
    registry: DatabaseModuleRegistry,
    applied_by: String,
}

impl RegistryLifecycleOrchestrator {
    pub fn new(
        pool: sdkwork_database_sqlx::DatabasePool,
        registry: DatabaseModuleRegistry,
    ) -> Self {
        Self {
            pool,
            registry,
            applied_by: "sdkwork-database-lifecycle".to_string(),
        }
    }

    pub async fn bootstrap_all(
        &self,
        locale: &LocaleTag,
        profile: &SeedProfile,
    ) -> Result<Vec<(String, usize, usize)>, LifecycleError> {
        let mut results = Vec::new();
        for module in self.registry.modules() {
            let orchestrator = LifecycleOrchestrator::new(self.pool.clone(), module.clone())
                .with_applied_by(&self.applied_by);
            let (migrations, seeds) = orchestrator.bootstrap(locale, profile).await?;
            results.push((module.descriptor().module_id.clone(), migrations, seeds));
        }
        Ok(results)
    }

    pub async fn migrate_all(&self) -> Result<Vec<(String, usize)>, LifecycleError> {
        let mut results = Vec::new();
        for module in self.registry.modules() {
            let orchestrator = LifecycleOrchestrator::new(self.pool.clone(), module.clone())
                .with_applied_by(&self.applied_by);
            let count = orchestrator.migrate().await?;
            results.push((module.descriptor().module_id.clone(), count));
        }
        Ok(results)
    }
}
