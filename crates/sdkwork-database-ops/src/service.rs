use sdkwork_database_drift::{DriftEngine, DriftReport};
use sdkwork_database_history::{
    fetch_installation_state, list_applied_migration_versions, list_applied_seeds,
};
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::{DatabaseModule, LocaleTag, SeedProfile};
use sdkwork_database_sqlx::DatabasePool;

use crate::error::OpsError;
use crate::types::{
    AppliedSeedEntry, DatabaseDriftResponse, DatabaseMigrationsReport, DatabaseSeedsReport,
    DatabaseStatusReport,
};

const MIN_DRIFT_REFRESH_INTERVAL_SECS: i64 = 5;

pub struct DatabaseOpsService {
    pool: DatabasePool,
    module: std::sync::Arc<dyn DatabaseModule>,
    cached_drift: std::sync::RwLock<Option<DriftReport>>,
    last_drift_refresh_at: std::sync::Mutex<Option<chrono::DateTime<chrono::Utc>>>,
}

impl DatabaseOpsService {
    pub fn new(pool: DatabasePool, module: std::sync::Arc<dyn DatabaseModule>) -> Self {
        Self {
            pool,
            module,
            cached_drift: std::sync::RwLock::new(None),
            last_drift_refresh_at: std::sync::Mutex::new(None),
        }
    }

    pub async fn status(&self) -> Result<DatabaseStatusReport, OpsError> {
        let descriptor = self.module.descriptor();
        let installation = fetch_installation_state(&self.pool, &descriptor.module_id).await?;
        let orchestrator = LifecycleOrchestrator::new(self.pool.clone(), self.module.clone());
        let pending = orchestrator.plan_migrations().await?;
        let drift = DriftEngine::new(self.pool.clone(), self.module.clone())
            .analyze()
            .await?;

        Ok(DatabaseStatusReport {
            schema_version: 1,
            kind: "sdkwork.database.ops-status".to_string(),
            module_id: descriptor.module_id,
            service_code: descriptor.service_code,
            engine: drift.engine.clone(),
            lifecycle_status: installation
                .as_ref()
                .map(|state| state.status.clone())
                .unwrap_or_else(|| "uninitialized".to_string()),
            contract_version: installation
                .as_ref()
                .and_then(|state| state.contract_version.clone()),
            seed_locale: installation
                .as_ref()
                .and_then(|state| state.seed_locale.clone()),
            seed_profile: installation
                .as_ref()
                .and_then(|state| state.seed_profile.clone()),
            pending_migrations: pending.len(),
            drift_status: drift.status,
        })
    }

    pub async fn drift(&self, refresh: bool) -> Result<DatabaseDriftResponse, OpsError> {
        if !refresh {
            if let Ok(guard) = self.cached_drift.read() {
                if let Some(cached) = guard.clone() {
                    return Ok(DatabaseDriftResponse { report: cached });
                }
            }
        } else if let Ok(guard) = self.last_drift_refresh_at.lock() {
            if let Some(last_refresh) = *guard {
                let elapsed = chrono::Utc::now().signed_duration_since(last_refresh);
                if elapsed.num_seconds() < MIN_DRIFT_REFRESH_INTERVAL_SECS {
                    if let Ok(cached_guard) = self.cached_drift.read() {
                        if let Some(cached) = cached_guard.clone() {
                            return Ok(DatabaseDriftResponse { report: cached });
                        }
                    }
                }
            }
        }

        let report = DriftEngine::new(self.pool.clone(), self.module.clone())
            .analyze()
            .await?;
        if let Ok(mut guard) = self.cached_drift.write() {
            *guard = Some(report.clone());
        }
        if refresh {
            if let Ok(mut guard) = self.last_drift_refresh_at.lock() {
                *guard = Some(chrono::Utc::now());
            }
        }
        Ok(DatabaseDriftResponse { report })
    }

    pub async fn migrations(&self) -> Result<DatabaseMigrationsReport, OpsError> {
        let descriptor = self.module.descriptor();
        let engine = self.pool.engine();
        let applied =
            list_applied_migration_versions(&self.pool, &descriptor.module_id, engine).await?;
        let orchestrator = LifecycleOrchestrator::new(self.pool.clone(), self.module.clone());
        let pending = orchestrator
            .plan_migrations()
            .await?
            .into_iter()
            .map(|migration| format!("{}_{}", migration.version, migration.name))
            .collect();

        Ok(DatabaseMigrationsReport {
            schema_version: 1,
            kind: "sdkwork.database.ops-migrations".to_string(),
            module_id: descriptor.module_id,
            applied,
            pending,
        })
    }

    pub async fn seeds(
        &self,
        locale: &LocaleTag,
        profile: &SeedProfile,
    ) -> Result<DatabaseSeedsReport, OpsError> {
        let descriptor = self.module.descriptor();
        let applied_records = list_applied_seeds(&self.pool, &descriptor.module_id).await?;
        let plan = self.module.resolve_seed_plan(locale, profile).await?;
        let mut pending = Vec::new();
        for script_path in plan.common_scripts.iter().chain(plan.locale_scripts.iter()) {
            let seed_id = script_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown")
                .to_string();
            let already_applied = applied_records.iter().any(|record| {
                record.seed_id == seed_id
                    && record.locale == locale.0
                    && record.profile == profile.0
            });
            if !already_applied {
                pending.push(seed_id);
            }
        }

        Ok(DatabaseSeedsReport {
            schema_version: 1,
            kind: "sdkwork.database.ops-seeds".to_string(),
            module_id: descriptor.module_id,
            applied: applied_records
                .into_iter()
                .map(|record| AppliedSeedEntry {
                    seed_id: record.seed_id,
                    locale: record.locale,
                    profile: record.profile,
                    checksum: record.checksum,
                })
                .collect(),
            pending,
        })
    }
}
