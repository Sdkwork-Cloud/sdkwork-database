use sdkwork_database_spi::{DatabaseManifest, DatabaseModuleRegistry, LocaleTag, SeedProfile};

use crate::error::LifecycleError;
use crate::options::lifecycle_options_from_env;
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

    /// Override the `applied_by` attribution recorded in migration/seed history.
    ///
    /// Federated hosts (e.g. ClawRouter) should set this to identify which
    /// integration context triggered the bootstrap (e.g.
    /// `"sdkwork-clawrouter-commerce"`).
    pub fn with_applied_by(mut self, applied_by: impl Into<String>) -> Self {
        self.applied_by = applied_by.into();
        self
    }

    /// Bootstrap all registered modules with explicit locale and profile.
    ///
    /// This method always runs init + migrate + seed for every module. Use
    /// [`bootstrap_all_from_env`](Self::bootstrap_all_from_env) instead when
    /// each module should respect its own manifest/env lifecycle options
    /// (auto_migrate, seed_on_boot, seed_locale, seed_profile).
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

    /// Bootstrap all registered modules, respecting per-module manifest/env
    /// lifecycle options.
    ///
    /// This is the **convention-over-configuration** entry point for
    /// federated assembly integration. Each module's manifest drives
    /// `auto_migrate`, `seed_on_boot`, `seed_locale`, and `seed_profile`,
    /// overridable through `SDKWORK_<SERVICE_CODE>_DATABASE_*` env vars.
    ///
    /// Federated hosts (e.g. ClawRouter) register all `*-database-host`
    /// modules into a `DatabaseModuleRegistry` and call this method once on
    /// the shared pool — no per-capability manual bootstrap wiring needed.
    pub async fn bootstrap_all_from_env(
        &self,
    ) -> Result<Vec<(String, usize, usize)>, LifecycleError> {
        let mut results = Vec::new();
        for module in self.registry.modules() {
            let descriptor = module.descriptor();
            let manifest = DatabaseManifest::from_file(module.manifest_path())?;
            let options = lifecycle_options_from_env(&descriptor.service_code, &manifest);
            let orchestrator = LifecycleOrchestrator::new(self.pool.clone(), module.clone())
                .with_applied_by(&self.applied_by);
            orchestrator.init().await?;
            let mut migrations = 0usize;
            if options.auto_migrate {
                migrations = orchestrator.migrate().await?;
            }
            let mut seeds = 0usize;
            if options.seed_on_boot {
                seeds = orchestrator
                    .seed(&options.seed_locale, &options.seed_profile)
                    .await?;
            }
            results.push((descriptor.module_id.clone(), migrations, seeds));
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
