use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::SpiError;
use crate::types::DriftPolicy;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftPolicyFile {
    pub schema_version: u32,
    pub kind: String,
    #[serde(default)]
    pub rules: DriftPolicyRules,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftPolicyRules {
    #[serde(default)]
    pub ignore_tables: Vec<String>,
    #[serde(default)]
    pub ignore_columns: Vec<String>,
    #[serde(default)]
    pub severity_overrides: HashMap<String, String>,
}

impl DriftPolicyFile {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, SpiError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|error| SpiError::Drift(format!("failed to read drift policy: {error}")))?;
        serde_yaml::from_str(&content)
            .map_err(|error| SpiError::Drift(format!("invalid drift policy yaml: {error}")))
    }

    pub fn into_policy(self) -> DriftPolicy {
        DriftPolicy {
            ignore_tables: self.rules.ignore_tables,
            ignore_columns: self.rules.ignore_columns,
            severity_overrides: self.rules.severity_overrides,
        }
    }
}
