use sdkwork_database_drift::DriftReport;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStatusReport {
    pub schema_version: u32,
    pub kind: String,
    pub module_id: String,
    pub service_code: String,
    pub engine: String,
    pub lifecycle_status: String,
    pub contract_version: Option<String>,
    pub seed_locale: Option<String>,
    pub seed_profile: Option<String>,
    pub pending_migrations: usize,
    pub drift_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseMigrationsReport {
    pub schema_version: u32,
    pub kind: String,
    pub module_id: String,
    pub applied: Vec<String>,
    pub pending: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedSeedEntry {
    pub seed_id: String,
    pub locale: String,
    pub profile: String,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSeedsReport {
    pub schema_version: u32,
    pub kind: String,
    pub module_id: String,
    pub applied: Vec<AppliedSeedEntry>,
    pub pending: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseDriftResponse {
    pub report: DriftReport,
}
