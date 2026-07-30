use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_history::{fetch_installation_state, InstallationState};
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::{DefaultDatabaseModule, LifecycleState, LocaleTag, SeedProfile};
use sdkwork_database_sqlx::{create_pool_from_config, DatabasePool};
use tempfile::TempDir;

const TEST_DATABASE_URL: &str = "SDKWORK_DATABASE_TEST_POSTGRES_URL";
const MODULE_ID: &str = "postgres_restart_state";

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

#[derive(Debug, PartialEq, Eq)]
struct StateSnapshot {
    contract_version: Option<String>,
    seed_locale: Option<String>,
    seed_profile: Option<String>,
    status: String,
}

impl From<InstallationState> for StateSnapshot {
    fn from(state: InstallationState) -> Self {
        Self {
            contract_version: state.contract_version,
            seed_locale: state.seed_locale,
            seed_profile: state.seed_profile,
            status: state.status,
        }
    }
}

struct LifecycleEvidence {
    current_schema: String,
    row_count: i64,
    baseline_marker_present: bool,
    bootstrapped: StateSnapshot,
    migrated: StateSnapshot,
    seeded: StateSnapshot,
    upgraded_init: StateSnapshot,
    restarted_init: StateSnapshot,
}

fn write_file(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().expect("test file parent"))?;
    std::fs::write(path, content)
}

fn write_module(root: &Path, contract_version: &str) -> std::io::Result<()> {
    write_file(
        &root.join("database/database.manifest.json"),
        &format!(
            r#"{{
  "schemaVersion": 1,
  "kind": "sdkwork.database.module",
  "moduleId": "{MODULE_ID}",
  "serviceCode": "RESTART_AUTHORITY",
  "tablePrefix": "restart_authority_",
  "contractVersion": "{contract_version}",
  "baselineStrategy": "baseline-plus-migrations",
  "baselineAnchorTable": "restart_authority_anchor",
  "paths": {{
    "contract": "contract/schema.yaml",
    "migrations": "migrations",
    "seeds": "seeds",
    "driftPolicy": "drift/policy.yaml"
  }},
  "lifecycle": {{ "activeSeedLocales": ["zh-CN"] }}
}}"#
        ),
    )?;
    write_file(
        &root.join("database/ddl/baseline/postgres/0001_restart_baseline.sql"),
        r#"CREATE TABLE restart_authority_anchor (id BIGINT PRIMARY KEY);
CREATE TABLE restart_authority_baseline_replayed (id BIGINT PRIMARY KEY);"#,
    )?;
    write_file(
        &root.join("database/migrations/postgres/0001_create_migration_probe.up.sql"),
        "CREATE TABLE restart_authority_migration_probe (id BIGINT PRIMARY KEY);",
    )?;
    write_file(
        &root.join("database/seeds/seed.manifest.json"),
        r#"{
  "schemaVersion": 1,
  "kind": "sdkwork.database.seed",
  "defaultLocale": "zh-CN",
  "profiles": {
    "standard": { "common": [], "locales": { "zh-CN": [] } }
  }
}"#,
    )
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

async fn create_postgres_pool(
    database_url: String,
) -> Result<DatabasePool, Box<dyn std::error::Error>> {
    Ok(create_pool_from_config(DatabaseConfig {
        engine: DatabaseEngine::Postgres,
        url: database_url,
        max_connections: 4,
        ..Default::default()
    })
    .await?)
}

async fn installation_state(
    pool: &DatabasePool,
) -> Result<StateSnapshot, Box<dyn std::error::Error>> {
    let state = fetch_installation_state(pool, MODULE_ID)
        .await?
        .ok_or_else(|| std::io::Error::other("installation state should exist"))?;
    Ok(state.into())
}

async fn drop_schema(admin: &sqlx::PgPool, schema: &str) -> Result<(), sqlx::Error> {
    let statement = format!("DROP SCHEMA IF EXISTS {schema} CASCADE");
    sqlx::query(&statement).execute(admin).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL with schema create/drop permission"]
async fn lifecycle_restart_preserves_advanced_state_and_pool_schema_authority() {
    let base_url =
        std::env::var(TEST_DATABASE_URL).expect("SDKWORK_DATABASE_TEST_POSTGRES_URL must be set");
    let actual_schema = unique_schema("database_lifecycle_actual");
    let decoy_schema = unique_schema("database_lifecycle_decoy");
    let admin = sqlx::PgPool::connect(&base_url)
        .await
        .expect("connect PostgreSQL test administrator");
    let temp = TempDir::new().expect("create temporary database module");

    let test_result: Result<LifecycleEvidence, Box<dyn std::error::Error>> = async {
        let create_actual = format!("CREATE SCHEMA {actual_schema}");
        let create_decoy = format!("CREATE SCHEMA {decoy_schema}");
        sqlx::query(&create_actual).execute(&admin).await?;
        sqlx::query(&create_decoy).execute(&admin).await?;
        let create_anchor = format!(
            "CREATE TABLE {actual_schema}.restart_authority_anchor (id BIGINT PRIMARY KEY)"
        );
        sqlx::query(&create_anchor).execute(&admin).await?;

        write_module(temp.path(), "1.0.0")?;
        let pool_url = postgres_url_with_schema(&base_url, &actual_schema)?;
        let pool = create_postgres_pool(pool_url.clone()).await?;
        let identity = pool
            .postgres_schema_identity()
            .await?
            .ok_or_else(|| std::io::Error::other("PostgreSQL identity should be available"))?;
        let current_schema = identity.current_schema().to_string();

        let _misleading_environment =
            EnvironmentVariableGuard::set("SDKWORK_DATABASE_SCHEMA", &decoy_schema);

        let first_module = Arc::new(DefaultDatabaseModule::from_app_root(temp.path())?);
        let second_module = Arc::new(DefaultDatabaseModule::from_app_root(temp.path())?);
        let first = LifecycleOrchestrator::new(pool.clone(), first_module);
        let second = LifecycleOrchestrator::new(pool.clone(), second_module);
        let (first_init, second_init) = tokio::join!(first.init(), second.init());
        first_init?;
        second_init?;

        let row_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM ops_database_installation_state WHERE module_id = $1",
        )
        .bind(MODULE_ID)
        .fetch_one(pool.as_postgres().expect("PostgreSQL pool"))
        .await?;
        let baseline_marker_present = sqlx::query_scalar::<_, bool>(
            "SELECT to_regclass('restart_authority_baseline_replayed') IS NOT NULL",
        )
        .fetch_one(pool.as_postgres().expect("PostgreSQL pool"))
        .await?;
        let bootstrapped = installation_state(&pool).await?;

        let module = Arc::new(DefaultDatabaseModule::from_app_root(temp.path())?);
        let orchestrator = LifecycleOrchestrator::new(pool.clone(), module);
        let migration_count = orchestrator.migrate().await?;
        if migration_count != 1 {
            return Err(std::io::Error::other(format!(
                "expected one migration, got {migration_count}"
            ))
            .into());
        }
        let migrated = installation_state(&pool).await?;
        orchestrator.init().await?;
        if installation_state(&pool).await? != migrated {
            return Err(std::io::Error::other(
                "init after migrate must preserve schema_current state",
            )
            .into());
        }

        let seed_count = orchestrator
            .seed(&LocaleTag::zh_cn(), &SeedProfile::standard())
            .await?;
        if seed_count != 0 {
            return Err(std::io::Error::other(format!(
                "expected an empty seed plan, got {seed_count} scripts"
            ))
            .into());
        }
        let seeded = installation_state(&pool).await?;
        orchestrator.init().await?;
        if installation_state(&pool).await? != seeded {
            return Err(std::io::Error::other("init after seed must preserve seeded state").into());
        }

        write_module(temp.path(), "2.0.0")?;
        let upgraded_module = Arc::new(DefaultDatabaseModule::from_app_root(temp.path())?);
        LifecycleOrchestrator::new(pool.clone(), upgraded_module)
            .init()
            .await?;
        let upgraded_init = installation_state(&pool).await?;
        pool.close().await;

        write_module(temp.path(), "3.0.0")?;
        let restarted_pool = create_postgres_pool(pool_url).await?;
        let restarted_module = Arc::new(DefaultDatabaseModule::from_app_root(temp.path())?);
        LifecycleOrchestrator::new(restarted_pool.clone(), restarted_module)
            .init()
            .await?;
        let restarted_init = installation_state(&restarted_pool).await?;
        restarted_pool.close().await;

        Ok(LifecycleEvidence {
            current_schema,
            row_count,
            baseline_marker_present,
            bootstrapped,
            migrated,
            seeded,
            upgraded_init,
            restarted_init,
        })
    }
    .await;

    let actual_cleanup = drop_schema(&admin, &actual_schema).await;
    let decoy_cleanup = drop_schema(&admin, &decoy_schema).await;
    admin.close().await;
    actual_cleanup.expect("drop actual schema");
    decoy_cleanup.expect("drop decoy schema");

    let evidence = test_result.expect("collect lifecycle restart evidence");
    assert_eq!(evidence.current_schema, actual_schema);
    assert_eq!(evidence.row_count, 1);
    assert!(!evidence.baseline_marker_present);
    assert_eq!(
        evidence.bootstrapped,
        StateSnapshot {
            contract_version: Some("1.0.0".to_string()),
            seed_locale: Some(String::new()),
            seed_profile: Some(String::new()),
            status: LifecycleState::Bootstrapped.status_label().to_string(),
        }
    );
    assert_eq!(
        evidence.migrated.status,
        LifecycleState::SchemaCurrent.status_label()
    );
    assert_eq!(evidence.migrated.contract_version.as_deref(), Some("1.0.0"));
    assert_eq!(
        evidence.seeded.status,
        LifecycleState::Seeded.status_label()
    );
    assert_eq!(evidence.seeded.contract_version.as_deref(), Some("1.0.0"));
    assert_eq!(evidence.seeded.seed_locale.as_deref(), Some("zh-CN"));
    assert_eq!(evidence.seeded.seed_profile.as_deref(), Some("standard"));
    assert_eq!(evidence.upgraded_init, evidence.seeded);
    assert_eq!(evidence.restarted_init, evidence.seeded);
}
