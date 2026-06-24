use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::SpiError;
use crate::types::{LocaleTag, SeedPlan, SeedProfile};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub kind: String,
    #[serde(default = "default_seed_locale")]
    pub default_locale: String,
    pub profiles: HashMap<String, SeedProfileDefinition>,
    #[serde(default)]
    pub supported_locales: Vec<String>,
    #[serde(default)]
    pub active_locales: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeedProfileDefinition {
    #[serde(default)]
    pub common: Vec<String>,
    #[serde(default)]
    pub locales: HashMap<String, Vec<String>>,
}

fn default_seed_locale() -> String {
    "zh-CN".to_string()
}

impl SeedManifest {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, SpiError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|error| SpiError::Seed(format!("failed to read seed manifest: {error}")))?;
        serde_json::from_str(&content)
            .map_err(|error| SpiError::Seed(format!("invalid seed manifest json: {error}")))
    }

    pub fn resolve_plan(
        &self,
        seeds_dir: &Path,
        locale: &LocaleTag,
        profile: &SeedProfile,
    ) -> Result<SeedPlan, SpiError> {
        let profile_def = self
            .profiles
            .get(&profile.0)
            .ok_or_else(|| SpiError::Seed(format!("unknown seed profile {}", profile.0)))?;

        let common_scripts = profile_def
            .common
            .iter()
            .map(|file| seeds_dir.join("common").join(file))
            .collect();

        let locale_scripts = profile_def
            .locales
            .get(&locale.0)
            .map(|files| {
                files
                    .iter()
                    .map(|file| seeds_dir.join("locales").join(&locale.0).join(file))
                    .collect()
            })
            .unwrap_or_default();

        Ok(SeedPlan {
            locale: locale.clone(),
            profile: profile.clone(),
            common_scripts,
            locale_scripts,
        })
    }
}
