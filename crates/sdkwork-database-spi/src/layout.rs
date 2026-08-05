use std::fs;
use std::path::Path;

use crate::manifest::DatabaseManifest;

const REQUIRED_LOCALES: &[&str] = &[
    "zh-CN", "en-US", "ja-JP", "de-DE", "fr-FR", "ru-RU", "ko-KR",
];

const REQUIRED_PATHS_COMMON: &[&str] = &[
    "README.md",
    "database.manifest.json",
    "contract/schema.yaml",
    "contract/prefix-registry.json",
    "contract/table-registry.json",
    "seeds/seed.manifest.json",
    "drift/policy.yaml",
    "seeds/common",
    "ddl/generated",
    "fixtures",
];

const POSTGRES_PATHS: &[&str] = &["migrations/postgres", "ddl/baseline/postgres"];

const SQLITE_PATHS: &[&str] = &["migrations/sqlite", "ddl/baseline/sqlite"];

const MIGRATION_NAME_PATTERN: &str = r"^\d{4}_[a-z0-9_]+\.up\.sql$";

/// Validates the standard module layout for a database module root.
///
/// The required engine directories are derived from the module manifest:
/// `authoritative-server` modules (engines `["postgres"]`) must provide the
/// postgres directories and MUST NOT contain sqlite engine directories;
/// `client-local` modules (engines `["sqlite"]`) must provide the sqlite
/// directories and MUST NOT contain postgres engine directories
/// (DATABASE_FRAMEWORK_SPEC.md §5.1/§5.2).
pub fn validate_module_layout(module_root: &Path) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();

    for relative in REQUIRED_PATHS_COMMON {
        let path = module_root.join(relative);
        if !path.exists() {
            failures.push(format!("{relative} must exist"));
        }
    }

    let manifest = DatabaseManifest::from_file(module_root.join("database.manifest.json")).ok();
    let is_client_local = manifest.as_ref().map_or(false, |module| {
        module.engines.iter().any(|engine| engine == "sqlite")
            || module.default_engine.as_deref() == Some("sqlite")
    });

    let (required_engine_paths, forbidden_engine_paths): (&[&str], &[&str]) = if is_client_local {
        (SQLITE_PATHS, POSTGRES_PATHS)
    } else {
        (POSTGRES_PATHS, SQLITE_PATHS)
    };

    for relative in required_engine_paths {
        if !module_root.join(relative).exists() {
            failures.push(format!("{relative} must exist"));
        }
    }
    for relative in forbidden_engine_paths {
        if module_root.join(relative).exists() {
            failures.push(format!("{relative} must not exist"));
        }
    }

    if is_client_local && !module_root.join("local-data-policy.yaml").exists() {
        failures.push("local-data-policy.yaml must exist for client-local modules".to_owned());
    }

    for locale in REQUIRED_LOCALES {
        let relative = format!("seeds/locales/{locale}");
        if !module_root.join(&relative).exists() {
            failures.push(format!("{relative} must exist"));
        }
    }

    failures.extend(validate_migration_filenames(module_root, is_client_local));

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn validate_migration_filenames(module_root: &Path, is_client_local: bool) -> Vec<String> {
    let mut failures = Vec::new();
    let pattern = regex::Regex::new(MIGRATION_NAME_PATTERN).expect("valid migration regex");
    let engine = if is_client_local {
        "sqlite"
    } else {
        "postgres"
    };

    let dir = module_root.join("migrations").join(engine);
    if !dir.exists() {
        return failures;
    }
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) => {
            failures.push(format!("migrations/{engine} unreadable: {error}"));
            return failures;
        }
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.ends_with(".sql") && !pattern.is_match(&name) {
            failures.push(format!(
                "migrations/{engine}/{name} must match {MIGRATION_NAME_PATTERN}"
            ));
        }
    }

    failures
}
