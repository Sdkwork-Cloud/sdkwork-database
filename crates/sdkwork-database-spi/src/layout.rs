use std::fs;
use std::path::Path;

const REQUIRED_LOCALES: &[&str] = &[
    "zh-CN", "en-US", "ja-JP", "de-DE", "fr-FR", "ru-RU", "ko-KR",
];

const REQUIRED_PATHS: &[&str] = &[
    "README.md",
    "database.manifest.json",
    "contract/schema.yaml",
    "contract/prefix-registry.json",
    "contract/table-registry.json",
    "seeds/seed.manifest.json",
    "drift/policy.yaml",
    "migrations/postgres",
    "migrations/sqlite",
    "seeds/common",
    "ddl/baseline/postgres",
    "ddl/baseline/sqlite",
    "ddl/generated",
    "fixtures",
];

const MIGRATION_NAME_PATTERN: &str = r"^\d{4}_[a-z0-9_]+\.up\.sql$";

pub fn validate_module_layout(module_root: &Path) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();

    for relative in REQUIRED_PATHS {
        let path = module_root.join(relative);
        if !path.exists() {
            failures.push(format!("{relative} must exist"));
        }
    }

    for locale in REQUIRED_LOCALES {
        let relative = format!("seeds/locales/{locale}");
        if !module_root.join(&relative).exists() {
            failures.push(format!("{relative} must exist"));
        }
    }

    failures.extend(validate_migration_filenames(module_root));

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn validate_migration_filenames(module_root: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    let pattern = regex::Regex::new(MIGRATION_NAME_PATTERN).expect("valid migration regex");

    for engine in ["postgres", "sqlite"] {
        let dir = module_root.join("migrations").join(engine);
        if !dir.exists() {
            continue;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                failures.push(format!("migrations/{engine} unreadable: {error}"));
                continue;
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
    }

    failures
}
