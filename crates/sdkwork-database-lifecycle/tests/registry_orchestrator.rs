use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_lifecycle::RegistryLifecycleOrchestrator;
use sdkwork_database_spi::{DatabaseModuleRegistry, DefaultDatabaseModule, LocaleTag, SeedProfile};
use sdkwork_database_sqlx::create_pool_from_config;
use tempfile::TempDir;

fn write_file(path: &std::path::Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn write_module(root: &std::path::Path, module_id: &str, table_name: &str) {
    let database_root = root.join(format!("database/modules/{module_id}"));
    write_file(
        &database_root.join("database.manifest.json"),
        &format!(
            r#"{{
  "schemaVersion": 1,
  "kind": "sdkwork.database.module",
  "moduleId": "{module_id}",
  "serviceCode": "DEMO",
  "tablePrefix": "{module_id}_",
  "contractVersion": "0.1.0",
  "paths": {{
    "contract": "contract/schema.yaml",
    "migrations": "migrations",
    "seeds": "seeds",
    "driftPolicy": "drift/policy.yaml"
  }},
  "lifecycle": {{ "activeSeedLocales": ["zh-CN"] }}
}}"#
        ),
    );
    write_file(
        &database_root.join("contract/schema.yaml"),
        &format!(
            "schema_version: 1\nkind: sdkwork.database.schema\nmodule_id: {module_id}\ncontract_version: 0.1.0\ntables: []\n"
        ),
    );
    write_file(
        &database_root.join("contract/table-registry.json"),
        &format!(
            r#"{{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{{"table_name":"{table_name}"}}]}}"#
        ),
    );
    write_file(
        &database_root.join(format!("migrations/sqlite/0001_create_{module_id}.up.sql")),
        &format!("CREATE TABLE {table_name} (id INTEGER PRIMARY KEY, label TEXT NOT NULL);"),
    );
    write_file(
        &database_root.join("seeds/seed.manifest.json"),
        r#"{
  "schemaVersion": 1,
  "kind": "sdkwork.database.seed",
  "defaultLocale": "zh-CN",
  "profiles": { "standard": { "common": [], "locales": { "zh-CN": [] } } }
}"#,
    );
    std::fs::create_dir_all(database_root.join("seeds/common")).unwrap();
    write_file(
        &database_root.join("drift/policy.yaml"),
        "schemaVersion: 1\nkind: sdkwork.database.drift-policy\nrules:\n  ignoreTables: []\n  ignoreColumns: []\n  severityOverrides: {}\n",
    );
}

#[tokio::test]
async fn registry_migrate_all_applies_each_module() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_module(root, "alpha", "alpha_probe");
    write_module(root, "beta", "beta_probe");

    let alpha =
        DefaultDatabaseModule::from_module_root(&root.join("database/modules/alpha")).unwrap();
    let beta =
        DefaultDatabaseModule::from_module_root(&root.join("database/modules/beta")).unwrap();
    let registry = DatabaseModuleRegistry::builder()
        .register(alpha)
        .unwrap()
        .register(beta)
        .unwrap()
        .build();

    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        ..Default::default()
    };
    let pool = create_pool_from_config(config).await.unwrap();
    let orchestrator = RegistryLifecycleOrchestrator::new(pool, registry);
    let results = orchestrator.migrate_all().await.unwrap();

    assert_eq!(results.len(), 2);
    assert!(results
        .iter()
        .any(|(module_id, count)| module_id == "alpha" && *count == 1));
    assert!(results
        .iter()
        .any(|(module_id, count)| module_id == "beta" && *count == 1));
}

#[tokio::test]
async fn registry_bootstrap_all_runs_init_and_seed() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_module(root, "alpha", "alpha_probe");

    let alpha =
        DefaultDatabaseModule::from_module_root(&root.join("database/modules/alpha")).unwrap();
    let registry = DatabaseModuleRegistry::builder()
        .register(alpha)
        .unwrap()
        .build();

    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        ..Default::default()
    };
    let pool = create_pool_from_config(config).await.unwrap();
    let orchestrator = RegistryLifecycleOrchestrator::new(pool, registry);
    let results = orchestrator
        .bootstrap_all(&LocaleTag::zh_cn(), &SeedProfile::standard())
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "alpha");
}
