use std::sync::Arc;

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_drift::DriftEngine;
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::DefaultDatabaseModule;
use sdkwork_database_sqlx::create_pool_from_config;
use tempfile::TempDir;

fn write_file(path: &std::path::Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[tokio::test]
async fn drift_clean_after_migrate() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    write_file(
        &root.join("database/database.manifest.json"),
        r#"{
  "schemaVersion": 1,
  "kind": "sdkwork.database.module",
  "moduleId": "demo",
  "serviceCode": "DEMO",
  "tablePrefix": "demo_",
  "contractVersion": "0.1.0",
  "paths": {
    "contract": "contract/schema.yaml",
    "migrations": "migrations",
    "seeds": "seeds",
    "driftPolicy": "drift/policy.yaml"
  },
  "lifecycle": { "activeSeedLocales": ["zh-CN"] }
}"#,
    );

    write_file(
        &root.join("database/contract/schema.yaml"),
        "schema_version: 1\nkind: sdkwork.database.schema\nmodule_id: demo\ncontract_version: 0.1.0\ntables: []\n",
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE demo_probe (id INTEGER PRIMARY KEY, label TEXT NOT NULL);",
    );

    let module = Arc::new(DefaultDatabaseModule::from_app_root(root).unwrap());
    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        ..Default::default()
    };
    let pool = create_pool_from_config(config).await.unwrap();
    let orchestrator = LifecycleOrchestrator::new(pool.clone(), module.clone());
    orchestrator.migrate().await.unwrap();

    let report = DriftEngine::new(pool, module).analyze().await.unwrap();
    assert_eq!(report.status, "clean");
    assert_eq!(report.summary.error, 0);
}
