use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub kind: String,
    #[serde(rename = "moduleId")]
    pub module_id: String,
    #[serde(rename = "serviceCode")]
    pub service_code: String,
    #[serde(rename = "tablePrefix")]
    pub table_prefix: String,
    #[serde(rename = "contractVersion")]
    pub contract_version: String,
    #[serde(default)]
    pub lifecycle: DatabaseManifestLifecycle,
    pub paths: DatabaseManifestPaths,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub engines: Vec<String>,
    #[serde(rename = "defaultEngine", default)]
    pub default_engine: Option<String>,
    #[serde(rename = "baselineStrategy", default)]
    pub baseline_strategy: Option<String>,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default)]
    pub spi: Option<DatabaseManifestSpi>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseManifestSpi {
    #[serde(default = "default_spi_provider")]
    pub provider: String,
    #[serde(default)]
    pub hooks: Vec<String>,
}

fn default_spi_provider() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseManifestLifecycle {
    #[serde(default)]
    pub auto_migrate: bool,
    #[serde(default)]
    pub seed_on_boot: bool,
    #[serde(default = "default_seed_locale")]
    pub default_seed_locale: String,
    #[serde(default = "default_seed_profile")]
    pub default_seed_profile: String,
    #[serde(default = "default_active_locales")]
    pub active_seed_locales: Vec<String>,
    #[serde(default = "default_drift_interval")]
    pub drift_check_interval_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseManifestPaths {
    pub contract: String,
    pub migrations: String,
    pub seeds: String,
    #[serde(rename = "driftPolicy")]
    pub drift_policy: String,
}

fn default_seed_locale() -> String {
    "zh-CN".to_string()
}

fn default_seed_profile() -> String {
    "standard".to_string()
}

fn default_active_locales() -> Vec<String> {
    vec!["zh-CN".to_string()]
}

fn default_drift_interval() -> u64 {
    60
}

impl DatabaseManifest {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, crate::SpiError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|error| {
            crate::SpiError::Manifest(format!("failed to read manifest: {error}"))
        })?;
        serde_json::from_str(&content)
            .map_err(|error| crate::SpiError::Manifest(format!("invalid manifest json: {error}")))
    }

    pub fn resolve_path(&self, root: impl AsRef<Path>, relative: &str) -> PathBuf {
        root.as_ref().join(relative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_seed_locale_is_zh_cn() {
        assert_eq!(default_seed_locale(), "zh-CN");
    }
}
