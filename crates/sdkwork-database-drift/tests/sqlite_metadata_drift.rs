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

fn write_manifest(root: &std::path::Path, module_id: &str, table_prefix: &str) {
    write_file(
        &root.join("database/database.manifest.json"),
        &format!(
            r#"{{
  "schemaVersion": 1,
  "kind": "sdkwork.database.module",
  "moduleId": "{module_id}",
  "serviceCode": "DRIFT_TEST",
  "tablePrefix": "{table_prefix}",
  "contractVersion": "0.1.0",
  "paths": {{
    "contract": "contract/schema.yaml",
    "migrations": "migrations",
    "seeds": "seeds",
    "driftPolicy": "drift/policy.yaml"
  }},
  "lifecycle": {{ "activeSeedLocales": ["zh-CN"] }}
}}"#,
        ),
    );
}

async fn analyze(root: &std::path::Path) -> sdkwork_database_drift::DriftReport {
    let module = Arc::new(DefaultDatabaseModule::from_app_root(root).unwrap());
    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        ..Default::default()
    };
    let pool = create_pool_from_config(config).await.unwrap();
    LifecycleOrchestrator::new(pool.clone(), module.clone())
        .migrate()
        .await
        .unwrap();
    DriftEngine::new(pool, module).analyze().await.unwrap()
}

#[tokio::test]
async fn ops_tables_and_canonical_sqlite_affinities_are_clean() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_manifest(root, "ops_drift_test", "ops_");
    write_file(
        &root.join("database/contract/schema.yaml"),
        r#"schema_version: 1
kind: sdkwork.database.schema
module_id: ops_drift_test
contract_version: 0.1.0
tables:
  - name: ops_probe
    columns:
      - { name: id, type: int64, required: true }
      - { name: metadata, type: json, required: true }
      - { name: amount, type: decimal(38, 12), required: true }
      - { name: enabled, type: bool, required: true }
      - { name: external_id, type: uuid, required: true }
    constraints:
      - { name: pk_ops_probe, type: primary_key, columns: [id] }
      - { name: ck_ops_probe_amount, type: check, columns: [amount] }
    indexes:
      - { name: idx_ops_probe_enabled, columns: [enabled, id] }
"#,
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"ops_probe"}]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE ops_probe (
  id BIGINT NOT NULL PRIMARY KEY,
  metadata TEXT NOT NULL,
  amount TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  external_id TEXT NOT NULL,
  CONSTRAINT ck_ops_probe_amount CHECK (length(amount) > 0)
);
CREATE INDEX idx_ops_probe_enabled ON ops_probe(enabled, id);",
    );

    let report = analyze(root).await;
    assert_eq!(
        report.status, "clean",
        "unexpected drift: {:?}",
        report.diffs
    );
    assert!(
        report.diffs.is_empty(),
        "unexpected drift: {:?}",
        report.diffs
    );
}

#[tokio::test]
async fn sqlite_primary_unique_and_foreign_keys_match_by_structure() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_manifest(root, "relation_drift_test", "relation_");
    write_file(
        &root.join("database/contract/schema.yaml"),
        r#"schema_version: 1
kind: sdkwork.database.schema
module_id: relation_drift_test
contract_version: 0.1.0
tables:
  - name: relation_parent
    columns:
      - { name: id, type: int64, required: true }
      - { name: code, type: string, required: true }
    constraints:
      - { name: pk_relation_parent, type: primary_key, columns: [id] }
      - { name: uk_relation_parent_code, type: unique, columns: [code] }
  - name: relation_child
    columns:
      - { name: id, type: int64, required: true }
      - { name: parent_id, type: int64, required: true }
    constraints:
      - { name: pk_relation_child, type: primary_key, columns: [id] }
      - name: fk_relation_child_parent
        type: foreign_key
        columns: [parent_id]
        references_table: relation_parent
        references_columns: [id]
"#,
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"relation_parent"},{"table_name":"relation_child"}]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_relations.up.sql"),
        "CREATE TABLE relation_parent (
  id BIGINT NOT NULL PRIMARY KEY,
  code TEXT NOT NULL,
  CONSTRAINT uk_relation_parent_code UNIQUE (code)
);
CREATE TABLE relation_child (
  id BIGINT NOT NULL PRIMARY KEY,
  parent_id INTEGER NOT NULL,
  CONSTRAINT fk_relation_child_parent FOREIGN KEY (parent_id)
    REFERENCES relation_parent (id)
);",
    );

    let report = analyze(root).await;
    assert_eq!(
        report.status, "clean",
        "unexpected drift: {:?}",
        report.diffs
    );
    assert!(
        report.diffs.is_empty(),
        "unexpected drift: {:?}",
        report.diffs
    );
}

#[tokio::test]
async fn sqlite_integer_primary_key_rowid_alias_is_reported() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_manifest(root, "rowid_drift_test", "rowid_");
    write_file(
        &root.join("database/contract/schema.yaml"),
        r#"schema_version: 1
kind: sdkwork.database.schema
module_id: rowid_drift_test
contract_version: 0.1.0
tables:
  - name: rowid_probe
    columns:
      - { name: id, type: int64, required: true }
      - { name: label, type: string, required: true }
"#,
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"rowid_probe"}]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE rowid_probe (
  id INTEGER NOT NULL PRIMARY KEY,
  label TEXT NOT NULL
);",
    );

    let report = analyze(root).await;
    let rowid_diff = report
        .diffs
        .iter()
        .find(|diff| diff.code == "sqlite_rowid_alias")
        .expect("INTEGER PRIMARY KEY must not be silently accepted as an int64 business id");
    assert_eq!(rowid_diff.severity, "error");
    assert!(rowid_diff.message.contains("rowid_probe.id"));
    assert_eq!(report.status, "drift_detected");
}

#[tokio::test]
async fn sqlite_matching_partial_unique_index_is_clean() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_manifest(root, "index_clean_test", "index_clean_");
    write_file(
        &root.join("database/contract/schema.yaml"),
        r#"schema_version: 1
kind: sdkwork.database.schema
module_id: index_clean_test
contract_version: 0.1.0
tables:
  - name: index_clean_probe
    columns:
      - { name: id, type: int64, required: true }
      - { name: tenant_id, type: int64, required: true }
      - { name: status, type: int32, required: true }
      - { name: deleted_at, type: instant, required: false }
    indexes:
      - name: uk_index_clean_active
        columns: [tenant_id, status]
        unique: true
        where: deleted_at IS NULL
"#,
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"index_clean_probe"}]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE index_clean_probe (
  id BIGINT NOT NULL PRIMARY KEY,
  tenant_id INTEGER NOT NULL,
  status INTEGER NOT NULL,
  deleted_at TEXT
);
CREATE UNIQUE INDEX uk_index_clean_active
  ON index_clean_probe (tenant_id, status)
  WHERE deleted_at IS NULL;",
    );

    let report = analyze(root).await;
    assert_eq!(
        report.status, "clean",
        "unexpected drift: {:?}",
        report.diffs
    );
    assert!(
        report.diffs.is_empty(),
        "unexpected drift: {:?}",
        report.diffs
    );
}

#[tokio::test]
async fn sqlite_same_named_index_with_wrong_structure_is_reported() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_manifest(root, "index_mismatch_test", "index_mismatch_");
    write_file(
        &root.join("database/contract/schema.yaml"),
        r#"schema_version: 1
kind: sdkwork.database.schema
module_id: index_mismatch_test
contract_version: 0.1.0
tables:
  - name: index_mismatch_probe
    columns:
      - { name: id, type: int64, required: true }
      - { name: tenant_id, type: int64, required: true }
      - { name: status, type: int32, required: true }
      - { name: deleted_at, type: instant, required: false }
    indexes:
      - name: uk_index_mismatch_active
        columns: [tenant_id, status]
        unique: true
        where: deleted_at IS NULL
"#,
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"index_mismatch_probe"}]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE index_mismatch_probe (
  id BIGINT NOT NULL PRIMARY KEY,
  tenant_id INTEGER NOT NULL,
  status INTEGER NOT NULL,
  deleted_at TEXT
);
CREATE INDEX uk_index_mismatch_active
  ON index_mismatch_probe (status, tenant_id);",
    );

    let report = analyze(root).await;
    let mismatch = report
        .diffs
        .iter()
        .find(|diff| diff.code == "index_mismatch")
        .expect("same-name wrong index must be reported");
    assert_eq!(mismatch.severity, "error");
    assert!(mismatch.message.contains("expected_unique=true"));
    assert!(mismatch.message.contains("actual_unique=false"));
    assert!(mismatch.message.contains("deleted_at IS NULL"));
}

#[tokio::test]
async fn sqlite_same_index_name_on_another_table_does_not_match() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_manifest(root, "index_table_test", "index_table_");
    write_file(
        &root.join("database/contract/schema.yaml"),
        r#"schema_version: 1
kind: sdkwork.database.schema
module_id: index_table_test
contract_version: 0.1.0
tables:
  - name: index_table_expected
    columns:
      - { name: id, type: int64, required: true }
      - { name: status, type: int32, required: true }
    indexes:
      - { name: idx_index_table_status, columns: [status] }
  - name: index_table_other
    columns:
      - { name: id, type: int64, required: true }
      - { name: status, type: int32, required: true }
"#,
    );
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"index_table_expected"},{"table_name":"index_table_other"}]}"#,
    );
    write_file(
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE index_table_expected (
  id BIGINT NOT NULL PRIMARY KEY,
  status INTEGER NOT NULL
);
CREATE TABLE index_table_other (
  id BIGINT NOT NULL PRIMARY KEY,
  status INTEGER NOT NULL
);
CREATE INDEX idx_index_table_status ON index_table_other (status);",
    );

    let report = analyze(root).await;
    assert!(report.diffs.iter().any(|diff| {
        diff.code == "missing_index" && diff.message.contains("index_table_expected")
    }));
    assert!(report
        .diffs
        .iter()
        .any(|diff| { diff.code == "extra_index" && diff.message.contains("index_table_other") }));
}
