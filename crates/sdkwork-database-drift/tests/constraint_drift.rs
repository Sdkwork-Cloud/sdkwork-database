use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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

fn unique_id() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    format!("{}_{}", pid, ts)
}

#[tokio::test]
async fn drift_detects_missing_constraint_and_extra_index() {
    let unique = unique_id();
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let module_id = format!("drift_test1_{}", unique);
    let db_path = temp.path().join(format!("test1_{}.db", unique));

    write_file(
        &root.join("database/database.manifest.json"),
        &format!(
            r#"{{
  "schemaVersion": 1,
  "kind": "sdkwork.database.module",
  "moduleId": "{}",
  "serviceCode": "TEST1",
  "tablePrefix": "test1_",
  "contractVersion": "0.1.0",
  "paths": {{
    "contract": "contract/schema.yaml",
    "migrations": "migrations",
    "seeds": "seeds",
    "driftPolicy": "drift/policy.yaml"
  }},
  "lifecycle": {{ "activeSeedLocales": ["zh-CN"] }}
}}"#,
            module_id
        ),
    );

    write_file(
        &root.join("database/contract/schema.yaml"),
        &format!(
            r#"schema_version: 1
kind: sdkwork.database.schema
module_id: {}
contract_version: 0.1.0
tables:
  - name: test1_probe
    columns:
      - {{ name: id, type: int64, required: true }}
      - {{ name: label, type: string, required: true }}
    constraints:
      - {{ name: uk_test1_probe_label, type: unique, columns: [label] }}
    indexes:
      - {{ name: idx_test1_probe_label, columns: [label] }}
"#,
            module_id
        ),
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"test1_probe"}]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE test1_probe (
  id INTEGER PRIMARY KEY,
  label TEXT NOT NULL,
  CONSTRAINT uk_test1_probe_label UNIQUE (label)
);
CREATE INDEX idx_test1_probe_label ON test1_probe(label);
CREATE INDEX idx_test1_probe_extra ON test1_probe(label);",
    );

    let module = Arc::new(DefaultDatabaseModule::from_app_root(root).unwrap());
    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: format!("sqlite:{}?mode=rwc", db_path.display()),
        max_connections: 1,
        ..Default::default()
    };
    let pool = create_pool_from_config(config).await.unwrap();
    let orchestrator = LifecycleOrchestrator::new(pool.clone(), module.clone());
    orchestrator.migrate().await.unwrap();

    let report = DriftEngine::new(pool, module).analyze().await.unwrap();
    assert!(
        report.diffs.iter().any(
            |diff| diff.code == "extra_index" && diff.message.contains("idx_test1_probe_extra")
        ),
        "expected extra_index drift: {:?}",
        report.diffs
    );
    assert!(
        !report
            .diffs
            .iter()
            .any(|diff| diff.code == "missing_constraint"),
        "table-level UNIQUE constraint should match its SQLite autoindex: {:?}",
        report.diffs
    );
}

#[tokio::test]
async fn drift_reports_missing_constraint_when_absent() {
    let unique = unique_id();
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let module_id = format!("drift_test2_{}", unique);
    let db_path = temp.path().join(format!("test2_{}.db", unique));

    write_file(
        &root.join("database/database.manifest.json"),
        &format!(
            r#"{{
  "schemaVersion": 1,
  "kind": "sdkwork.database.module",
  "moduleId": "{}",
  "serviceCode": "TEST2",
  "tablePrefix": "test2_",
  "contractVersion": "0.1.0",
  "paths": {{
    "contract": "contract/schema.yaml",
    "migrations": "migrations",
    "seeds": "seeds",
    "driftPolicy": "drift/policy.yaml"
  }},
  "lifecycle": {{ "activeSeedLocales": ["zh-CN"] }}
}}"#,
            module_id
        ),
    );

    write_file(
        &root.join("database/contract/schema.yaml"),
        &format!(
            r#"schema_version: 1
kind: sdkwork.database.schema
module_id: {}
contract_version: 0.1.0
tables:
  - name: test2_probe
    columns:
      - {{ name: id, type: int64, required: true }}
      - {{ name: label, type: string, required: true }}
    constraints:
      - {{ name: uk_test2_probe_label, type: unique, columns: [label] }}
"#,
            module_id
        ),
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"test2_probe"}]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE test2_probe (id INTEGER PRIMARY KEY, label TEXT NOT NULL);",
    );

    let module = Arc::new(DefaultDatabaseModule::from_app_root(root).unwrap());
    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: format!("sqlite:{}?mode=rwc", db_path.display()),
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
