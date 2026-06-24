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
async fn drift_detects_missing_constraint_and_extra_index() {
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
        r#"schema_version: 1
kind: sdkwork.database.schema
module_id: demo
contract_version: 0.1.0
tables:
  - name: demo_probe
    columns:
      - { name: id, type: int64, required: true }
      - { name: label, type: string, required: true }
    constraints:
      - { name: uk_demo_probe_label, type: unique, columns: [label] }
    indexes:
      - { name: idx_demo_probe_label, columns: [label] }
"#,
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"demo_probe"}]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE demo_probe (
  id INTEGER PRIMARY KEY,
  label TEXT NOT NULL,
  CONSTRAINT uk_demo_probe_label UNIQUE (label)
);
CREATE INDEX idx_demo_probe_label ON demo_probe(label);
CREATE INDEX idx_demo_probe_extra ON demo_probe(label);",
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
    assert!(
        report
            .diffs
            .iter()
            .any(|diff| diff.code == "extra_index" && diff.message.contains("idx_demo_probe_extra")),
        "expected extra_index drift: {:?}",
        report.diffs
    );
}

#[tokio::test]
async fn drift_reports_missing_constraint_when_absent() {
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
        r#"schema_version: 1
kind: sdkwork.database.schema
module_id: demo
contract_version: 0.1.0
tables:
  - name: demo_probe
    columns:
      - { name: id, type: int64, required: true }
      - { name: label, type: string, required: true }
    constraints:
      - { name: uk_demo_probe_label, type: unique, columns: [label] }
"#,
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"demo_probe"}]}"#,
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
    assert!(
        report
            .diffs
            .iter()
            .any(|diff| diff.code == "missing_constraint"),
        "expected missing_constraint drift: {:?}",
        report.diffs
    );
}
