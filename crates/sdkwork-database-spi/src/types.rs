use std::path::PathBuf;

use sdkwork_database_config::DatabaseEngine;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleTag(pub String);

impl LocaleTag {
    pub fn zh_cn() -> Self {
        Self("zh-CN".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedProfile(pub String);

impl SeedProfile {
    pub fn standard() -> Self {
        Self("standard".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Uninitialized,
    Bootstrapped,
    SchemaCurrent,
    Seeded,
    Operational,
    DriftDetected,
    Migrating,
    Seeding,
    Failed,
}

impl LifecycleState {
    pub fn status_label(self) -> &'static str {
        match self {
            Self::Uninitialized => "uninitialized",
            Self::Bootstrapped => "bootstrapped",
            Self::SchemaCurrent => "schema_current",
            Self::Seeded => "seeded",
            Self::Operational => "operational",
            Self::DriftDetected => "drift_detected",
            Self::Migrating => "migrating",
            Self::Seeding => "seeding",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseModuleDescriptor {
    pub module_id: String,
    pub service_code: String,
    pub table_prefix: String,
    pub supported_engines: Vec<DatabaseEngine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSpec {
    pub version: String,
    pub name: String,
    pub engine: DatabaseEngine,
    pub up_path: PathBuf,
    pub down_path: Option<PathBuf>,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationContext {
    pub module_id: String,
    pub engine: DatabaseEngine,
    pub migration: MigrationSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedPlan {
    pub locale: LocaleTag,
    pub profile: SeedProfile,
    pub common_scripts: Vec<PathBuf>,
    pub locale_scripts: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedContext {
    pub module_id: String,
    pub plan: SeedPlan,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriftPolicy {
    pub ignore_tables: Vec<String>,
    pub ignore_columns: Vec<String>,
    #[serde(default)]
    pub severity_overrides: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleOptions {
    pub auto_migrate: bool,
    pub seed_on_boot: bool,
    pub seed_locale: LocaleTag,
    pub seed_profile: SeedProfile,
    pub drift_interval_sec: u64,
}

impl Default for LifecycleOptions {
    fn default() -> Self {
        Self {
            auto_migrate: false,
            seed_on_boot: false,
            seed_locale: LocaleTag::zh_cn(),
            seed_profile: SeedProfile::standard(),
            drift_interval_sec: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleStateEvent {
    pub module_id: String,
    pub from: LifecycleState,
    pub to: LifecycleState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleFailureEvent {
    pub module_id: String,
    pub state: LifecycleState,
    pub message: String,
}
