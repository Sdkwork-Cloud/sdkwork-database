use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

fn normalize_seed_entry(entry: &str) -> &str {
    entry.trim_start_matches("./")
}

fn resolve_common_script_path(seeds_dir: &Path, file: &str) -> PathBuf {
    let relative = Path::new(normalize_seed_entry(file));
    if relative
        .components()
        .next()
        .is_some_and(|component| component.as_os_str() == "common")
    {
        seeds_dir.join(relative)
    } else {
        seeds_dir.join("common").join(relative)
    }
}

fn resolve_locale_script_path(seeds_dir: &Path, locale: &str, file: &str) -> PathBuf {
    let relative = Path::new(normalize_seed_entry(file));
    let mut components = relative.components();
    let first = components.next();
    let second = components.next();
    if first.is_some_and(|component| component.as_os_str() == "locales")
        && second.is_some_and(|component| component.as_os_str() == locale)
    {
        seeds_dir.join(relative)
    } else {
        seeds_dir.join("locales").join(locale).join(relative)
    }
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
            .map(|file| resolve_common_script_path(seeds_dir, file))
            .collect();

        let locale_scripts = profile_def
            .locales
            .get(&locale.0)
            .map(|files| {
                files
                    .iter()
                    .map(|file| resolve_locale_script_path(seeds_dir, &locale.0, file))
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

#[cfg(test)]
mod resolve_seed_path_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn resolves_common_script_filename_relative_to_common_dir() {
        let seeds_dir = PathBuf::from("/app/database/seeds");
        assert_eq!(
            resolve_common_script_path(&seeds_dir, "001_bootstrap.sql"),
            PathBuf::from("/app/database/seeds/common/001_bootstrap.sql")
        );
    }

    #[test]
    fn resolves_common_script_path_prefixed_with_common_dir() {
        let seeds_dir = PathBuf::from("/app/database/seeds");
        assert_eq!(
            resolve_common_script_path(&seeds_dir, "common/001_bootstrap.sql"),
            PathBuf::from("/app/database/seeds/common/001_bootstrap.sql")
        );
    }

    #[test]
    fn resolves_locale_script_filename_relative_to_locale_dir() {
        let seeds_dir = PathBuf::from("/app/database/seeds");
        assert_eq!(
            resolve_locale_script_path(&seeds_dir, "zh-CN", "001_roles.sql"),
            PathBuf::from("/app/database/seeds/locales/zh-CN/001_roles.sql")
        );
    }

    #[test]
    fn resolves_locale_script_path_prefixed_with_locales_dir() {
        let seeds_dir = PathBuf::from("/app/database/seeds");
        assert_eq!(
            resolve_locale_script_path(&seeds_dir, "zh-CN", "locales/zh-CN/001_roles.sql"),
            PathBuf::from("/app/database/seeds/locales/zh-CN/001_roles.sql")
        );
    }
}
