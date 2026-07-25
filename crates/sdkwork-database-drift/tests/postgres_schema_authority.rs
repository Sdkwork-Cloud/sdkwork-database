use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_drift::introspect::{
    introspect_table_column_details, introspect_table_constraint_details,
    introspect_table_index_details, introspect_tables,
};
use sdkwork_database_drift::{DriftEngine, DriftReport};
use sdkwork_database_history::ensure_history_tables;
use sdkwork_database_spi::DefaultDatabaseModule;
use sdkwork_database_sqlx::create_pool_from_config;
use tempfile::TempDir;

const TEST_DATABASE_URL: &str = "SDKWORK_DATABASE_TEST_POSTGRES_URL";

struct EnvironmentVariableGuard {
    key: &'static str,
    previous_value: Option<OsString>,
}

impl EnvironmentVariableGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous_value = std::env::var_os(key);
        std::env::set_var(key, value);
        Self {
            key,
            previous_value,
        }
    }
}

impl Drop for EnvironmentVariableGuard {
    fn drop(&mut self) {
        match &self.previous_value {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

struct DriftEvidence {
    current_schema: String,
    search_path: String,
    tables: Vec<String>,
    probe_columns: Vec<String>,
    probe_indexes: Vec<String>,
    probe_constraints: Vec<String>,
    report: DriftReport,
}

fn write_file(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().expect("test file parent"))?;
    std::fs::write(path, content)
}

fn unique_schema(prefix: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the Unix epoch")
        .as_nanos();
    format!("{prefix}_{}_{}", std::process::id(), timestamp)
}

fn postgres_url_with_schema(base_url: &str, schema: &str) -> Result<String, url::ParseError> {
    let mut url = url::Url::parse(base_url)?;
    let mut retained_pairs = Vec::new();
    let mut postgres_options = Vec::new();
    for (key, value) in url.query_pairs() {
        if key.eq_ignore_ascii_case("options") {
            postgres_options.push(value.into_owned());
        } else {
            retained_pairs.push((key.into_owned(), value.into_owned()));
        }
    }
    postgres_options.push(format!("-c search_path={schema}"));
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in retained_pairs {
            query.append_pair(&key, &value);
        }
        query.append_pair("options", &postgres_options.join(" "));
    }
    Ok(url.into())
}

fn write_drift_module(root: &Path) -> std::io::Result<()> {
    write_file(
        &root.join("database/database.manifest.json"),
        r#"{
  "schemaVersion": 1,
  "kind": "sdkwork.database.module",
  "moduleId": "postgres_schema_authority",
  "serviceCode": "SCHEMA_AUTHORITY",
  "tablePrefix": "schema_authority_",
  "contractVersion": "1.0.0",
  "paths": {
    "contract": "contract/schema.yaml",
    "migrations": "migrations",
    "seeds": "seeds",
    "driftPolicy": "drift/policy.yaml"
  },
  "lifecycle": { "activeSeedLocales": ["zh-CN"] }
}"#,
    )?;
    write_file(
        &root.join("database/contract/schema.yaml"),
        r#"schema_version: 1
kind: sdkwork.database.schema
module_id: postgres_schema_authority
contract_version: 1.0.0
tables:
  - name: schema_authority_probe
    columns:
      - { name: id, type: int64, required: true }
      - { name: label, type: string, required: true }
      - { name: payload, type: string, required: true }
    indexes:
      - { name: idx_schema_authority_decoy_payload, columns: [payload] }
    constraints:
      - { name: uq_schema_authority_decoy_payload, type: unique, columns: [payload] }
"#,
    )?;
    write_file(
        &root.join("database/contract/table-registry.json"),
        r#"{"schemaVersion":1,"kind":"sdkwork.database.table-registry","tables":[{"table_name":"schema_authority_probe"}]}"#,
    )
}

async fn drop_schema(admin: &sqlx::PgPool, schema: &str) -> Result<(), sqlx::Error> {
    let statement = format!("DROP SCHEMA IF EXISTS {schema} CASCADE");
    sqlx::query(&statement).execute(admin).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL with schema create/drop permission"]
async fn drift_uses_the_established_pool_schema_after_environment_changes() {
    let base_url =
        std::env::var(TEST_DATABASE_URL).expect("SDKWORK_DATABASE_TEST_POSTGRES_URL must be set");
    let actual_schema = unique_schema("database_drift_actual");
    let decoy_schema = unique_schema("database_drift_decoy");
    let admin = sqlx::PgPool::connect(&base_url)
        .await
        .expect("connect PostgreSQL test administrator");

    let create_actual = format!("CREATE SCHEMA {actual_schema}");
    let create_decoy = format!("CREATE SCHEMA {decoy_schema}");
    sqlx::query(&create_actual)
        .execute(&admin)
        .await
        .expect("create actual schema");
    sqlx::query(&create_decoy)
        .execute(&admin)
        .await
        .expect("create decoy schema");

    let actual_ddl = format!(
        "CREATE TABLE {actual_schema}.schema_authority_probe (\
             id BIGINT PRIMARY KEY, \
             label TEXT NOT NULL, \
             CONSTRAINT uq_schema_authority_actual_label UNIQUE (label)\
         ); \
         CREATE INDEX idx_schema_authority_actual_label \
             ON {actual_schema}.schema_authority_probe (label); \
         CREATE TABLE {actual_schema}.schema_authority_actual_marker (id BIGINT PRIMARY KEY);"
    );
    let decoy_ddl = format!(
        "CREATE TABLE {decoy_schema}.schema_authority_probe (\
             id BIGINT PRIMARY KEY, \
             label TEXT NOT NULL, \
             payload TEXT NOT NULL, \
             CONSTRAINT uq_schema_authority_decoy_payload UNIQUE (payload)\
         ); \
         CREATE INDEX idx_schema_authority_decoy_payload \
             ON {decoy_schema}.schema_authority_probe (payload); \
         CREATE TABLE {decoy_schema}.schema_authority_decoy_marker (id BIGINT PRIMARY KEY);"
    );
    sqlx::raw_sql(&actual_ddl)
        .execute(&admin)
        .await
        .expect("create actual schema fixtures");
    sqlx::raw_sql(&decoy_ddl)
        .execute(&admin)
        .await
        .expect("create decoy schema fixtures");

    let pool_url = postgres_url_with_schema(&base_url, &actual_schema)
        .expect("build actual-schema PostgreSQL URL");
    let pool = create_pool_from_config(DatabaseConfig {
        engine: DatabaseEngine::Postgres,
        url: pool_url,
        max_connections: 4,
        ..Default::default()
    })
    .await
    .expect("create schema-authority pool");

    let _misleading_environment = [
        EnvironmentVariableGuard::set("SDKWORK_SCHEMA_AUTHORITY_DATABASE_SCHEMA", &decoy_schema),
        EnvironmentVariableGuard::set("SDKWORK_CLAW_DATABASE_SCHEMA", &decoy_schema),
        EnvironmentVariableGuard::set("SDKWORK_DATABASE_SCHEMA", &decoy_schema),
    ];
    let temp = TempDir::new().expect("create temporary database module");
    write_drift_module(temp.path()).expect("write drift module");

    let test_result: Result<DriftEvidence, Box<dyn std::error::Error>> = async {
        ensure_history_tables(&pool).await?;
        let identity = pool
            .postgres_schema_identity()
            .await?
            .ok_or_else(|| std::io::Error::other("PostgreSQL identity should be available"))?;
        let tables = introspect_tables(&pool).await?;
        let columns = introspect_table_column_details(&pool).await?;
        let indexes = introspect_table_index_details(&pool).await?;
        let constraints = introspect_table_constraint_details(&pool).await?;
        let module = Arc::new(DefaultDatabaseModule::from_app_root(temp.path())?);
        let report = DriftEngine::new(pool.clone(), module).analyze().await?;

        Ok(DriftEvidence {
            current_schema: identity.current_schema().to_string(),
            search_path: identity.search_path().to_string(),
            tables,
            probe_columns: columns
                .get("schema_authority_probe")
                .into_iter()
                .flatten()
                .map(|column| column.name.clone())
                .collect(),
            probe_indexes: indexes
                .get("schema_authority_probe")
                .into_iter()
                .flatten()
                .map(|index| index.name.clone())
                .collect(),
            probe_constraints: constraints
                .get("schema_authority_probe")
                .into_iter()
                .flatten()
                .filter_map(|constraint| constraint.name.clone())
                .collect(),
            report,
        })
    }
    .await;

    pool.close().await;
    let actual_cleanup = drop_schema(&admin, &actual_schema).await;
    let decoy_cleanup = drop_schema(&admin, &decoy_schema).await;
    admin.close().await;
    actual_cleanup.expect("drop actual schema");
    decoy_cleanup.expect("drop decoy schema");

    let evidence = test_result.expect("collect drift schema-authority evidence");
    assert_eq!(evidence.current_schema, actual_schema);
    assert_eq!(evidence.search_path, actual_schema);
    assert!(evidence
        .tables
        .contains(&"schema_authority_actual_marker".to_string()));
    assert!(!evidence
        .tables
        .contains(&"schema_authority_decoy_marker".to_string()));
    assert_eq!(evidence.probe_columns, ["id", "label"]);
    assert!(evidence
        .probe_indexes
        .contains(&"idx_schema_authority_actual_label".to_string()));
    assert!(!evidence
        .probe_indexes
        .contains(&"idx_schema_authority_decoy_payload".to_string()));
    assert!(evidence
        .probe_constraints
        .contains(&"uq_schema_authority_actual_label".to_string()));
    assert!(!evidence
        .probe_constraints
        .contains(&"uq_schema_authority_decoy_payload".to_string()));
    assert_eq!(evidence.report.live_tables, evidence.tables);
    assert!(
        evidence.report.diffs.iter().any(|diff| {
            diff.code == "missing_column" && diff.message.contains("schema_authority_probe.payload")
        }),
        "the decoy schema must not make the actual schema appear clean: {:?}",
        evidence.report.diffs
    );
}
