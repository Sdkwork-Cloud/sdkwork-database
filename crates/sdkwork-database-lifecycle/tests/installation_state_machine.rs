use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_history::{
    fetch_installation_state, upsert_installation_state, InstallationState,
};
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::traits::DatabaseModuleDescriptorProvider;
use sdkwork_database_spi::{
    DatabaseAssetProvider, DatabaseContractProvider, DatabaseLifecycleListener, DatabaseModule,
    DatabaseModuleDescriptor, DefaultDatabaseModule, DriftPolicy, DriftPolicyProvider,
    LifecycleState, LifecycleStateEvent, LocaleTag, MigrationProvider, MigrationSpec, SeedPlan,
    SeedProfile, SeedProvider, SpiError,
};
use sdkwork_database_sqlx::{create_pool_from_config, DatabasePool};
use tempfile::TempDir;

fn write_file(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().expect("test file parent"))
        .expect("create test directory");
    std::fs::write(path, content).expect("write test file");
}

fn write_module(root: &Path, contract_version: &str, with_migration_and_seed: bool) {
    write_file(
        &root.join("database/database.manifest.json"),
        &format!(
            r#"{{
  "schemaVersion": 1,
  "kind": "sdkwork.database.module",
  "moduleId": "state_machine",
  "serviceCode": "STATE_MACHINE",
  "tablePrefix": "state_machine_",
  "contractVersion": "{contract_version}",
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
    if with_migration_and_seed {
        write_file(
            &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
            "CREATE TABLE state_machine_probe (id INTEGER PRIMARY KEY);",
        );
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
        );
    }
}

async fn sqlite_pool(url: String, max_connections: u32) -> DatabasePool {
    create_pool_from_config(DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url,
        max_connections,
        ..Default::default()
    })
    .await
    .expect("create SQLite pool")
}

#[derive(Clone)]
struct RecordingListener {
    events: Arc<Mutex<Vec<LifecycleStateEvent>>>,
}

#[async_trait]
impl DatabaseLifecycleListener for RecordingListener {
    async fn on_state_change(&self, event: LifecycleStateEvent) -> Result<(), SpiError> {
        self.events
            .lock()
            .expect("listener events should lock")
            .push(event);
        Ok(())
    }
}

#[derive(Clone)]
struct ListeningModule {
    inner: DefaultDatabaseModule,
    events: Arc<Mutex<Vec<LifecycleStateEvent>>>,
}

impl ListeningModule {
    fn from_app_root(
        root: &Path,
        events: Arc<Mutex<Vec<LifecycleStateEvent>>>,
    ) -> Result<Self, SpiError> {
        Ok(Self {
            inner: DefaultDatabaseModule::from_app_root(root)?,
            events,
        })
    }
}

impl DatabaseModuleDescriptorProvider for ListeningModule {
    fn descriptor(&self) -> DatabaseModuleDescriptor {
        self.inner.descriptor()
    }
}

#[async_trait]
impl DatabaseAssetProvider for ListeningModule {
    fn module_root(&self) -> &Path {
        self.inner.module_root()
    }

    fn manifest_path(&self) -> PathBuf {
        self.inner.manifest_path()
    }

    fn contract_path(&self) -> PathBuf {
        self.inner.contract_path()
    }

    fn migrations_dir(&self, engine: DatabaseEngine) -> PathBuf {
        self.inner.migrations_dir(engine)
    }

    fn seeds_dir(&self) -> PathBuf {
        self.inner.seeds_dir()
    }

    fn drift_policy_path(&self) -> PathBuf {
        self.inner.drift_policy_path()
    }
}

#[async_trait]
impl DatabaseContractProvider for ListeningModule {
    async fn contract_version(&self) -> Result<String, SpiError> {
        self.inner.contract_version().await
    }
}

#[async_trait]
impl MigrationProvider for ListeningModule {
    async fn list_migrations(
        &self,
        engine: DatabaseEngine,
    ) -> Result<Vec<MigrationSpec>, SpiError> {
        self.inner.list_migrations(engine).await
    }
}

#[async_trait]
impl SeedProvider for ListeningModule {
    async fn resolve_seed_plan(
        &self,
        locale: &LocaleTag,
        profile: &SeedProfile,
    ) -> Result<SeedPlan, SpiError> {
        self.inner.resolve_seed_plan(locale, profile).await
    }
}

#[async_trait]
impl DriftPolicyProvider for ListeningModule {
    async fn load_policy(&self) -> Result<DriftPolicy, SpiError> {
        self.inner.load_policy().await
    }
}

#[async_trait]
impl DatabaseModule for ListeningModule {
    fn listeners(&self) -> Vec<Box<dyn DatabaseLifecycleListener>> {
        vec![Box::new(RecordingListener {
            events: Arc::clone(&self.events),
        })]
    }
}

async fn installation_state(pool: &DatabasePool) -> InstallationState {
    fetch_installation_state(pool, "state_machine")
        .await
        .expect("fetch installation state")
        .expect("installation state should exist")
}

#[tokio::test]
async fn init_is_insert_only_and_preserves_advanced_state() {
    let temp = TempDir::new().expect("temporary module root");
    write_module(temp.path(), "1.0.0", false);
    let events = Arc::new(Mutex::new(Vec::new()));
    let pool = sqlite_pool("sqlite::memory:".to_string(), 1).await;
    let module = Arc::new(
        ListeningModule::from_app_root(temp.path(), Arc::clone(&events))
            .expect("load listening module"),
    );
    let orchestrator = LifecycleOrchestrator::new(pool.clone(), module);

    orchestrator.init().await.expect("first init");
    orchestrator.init().await.expect("repeat init");
    let initial = installation_state(&pool).await;
    assert_eq!(initial.status, LifecycleState::Bootstrapped.status_label());
    assert_eq!(initial.contract_version.as_deref(), Some("1.0.0"));
    {
        let recorded = events.lock().expect("listener events should lock");
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].from, LifecycleState::Uninitialized);
        assert_eq!(recorded[0].to, LifecycleState::Bootstrapped);
    }

    upsert_installation_state(
        &pool,
        "state_machine",
        "1.0.0",
        "",
        "",
        LifecycleState::SchemaCurrent.status_label(),
    )
    .await
    .expect("record schema-current state");
    write_module(temp.path(), "2.0.0", false);
    let upgraded_module = Arc::new(
        ListeningModule::from_app_root(temp.path(), Arc::clone(&events))
            .expect("reload upgraded module"),
    );
    LifecycleOrchestrator::new(pool.clone(), upgraded_module)
        .init()
        .await
        .expect("init after manifest upgrade");
    let schema_current = installation_state(&pool).await;
    assert_eq!(
        schema_current.status,
        LifecycleState::SchemaCurrent.status_label()
    );
    assert_eq!(schema_current.contract_version.as_deref(), Some("1.0.0"));

    upsert_installation_state(
        &pool,
        "state_machine",
        "1.0.0",
        "zh-CN",
        "standard",
        LifecycleState::Seeded.status_label(),
    )
    .await
    .expect("record seeded state");
    write_module(temp.path(), "3.0.0", false);
    let upgraded_module = Arc::new(
        ListeningModule::from_app_root(temp.path(), Arc::clone(&events))
            .expect("reload second upgraded module"),
    );
    LifecycleOrchestrator::new(pool.clone(), upgraded_module)
        .init()
        .await
        .expect("init after seeded restart");
    let seeded = installation_state(&pool).await;
    assert_eq!(seeded.status, LifecycleState::Seeded.status_label());
    assert_eq!(seeded.contract_version.as_deref(), Some("1.0.0"));
    assert_eq!(seeded.seed_locale.as_deref(), Some("zh-CN"));
    assert_eq!(seeded.seed_profile.as_deref(), Some("standard"));
    assert_eq!(events.lock().expect("listener events should lock").len(), 1);
}

#[tokio::test]
async fn migrate_and_seed_are_the_only_phase_state_writers() {
    let temp = TempDir::new().expect("temporary module root");
    write_module(temp.path(), "4.2.0", true);
    let pool = sqlite_pool("sqlite::memory:".to_string(), 1).await;
    let module =
        Arc::new(DefaultDatabaseModule::from_app_root(temp.path()).expect("load database module"));
    let orchestrator = LifecycleOrchestrator::new(pool.clone(), module);

    assert_eq!(orchestrator.migrate().await.expect("migrate"), 1);
    let migrated = installation_state(&pool).await;
    assert_eq!(
        migrated.status,
        LifecycleState::SchemaCurrent.status_label()
    );
    assert_eq!(migrated.contract_version.as_deref(), Some("4.2.0"));
    orchestrator
        .init()
        .await
        .expect("restart init after migrate");
    let migrated_after_init = installation_state(&pool).await;
    assert_eq!(migrated_after_init.status, migrated.status);
    assert_eq!(
        migrated_after_init.contract_version,
        migrated.contract_version
    );

    assert_eq!(
        orchestrator
            .seed(&LocaleTag::zh_cn(), &SeedProfile::standard())
            .await
            .expect("seed"),
        0
    );
    let seeded = installation_state(&pool).await;
    assert_eq!(seeded.status, LifecycleState::Seeded.status_label());
    assert_eq!(seeded.contract_version.as_deref(), Some("4.2.0"));
    assert_eq!(seeded.seed_locale.as_deref(), Some("zh-CN"));
    assert_eq!(seeded.seed_profile.as_deref(), Some("standard"));
    orchestrator.init().await.expect("restart init after seed");
    let seeded_after_init = installation_state(&pool).await;
    assert_eq!(seeded_after_init.status, seeded.status);
    assert_eq!(seeded_after_init.contract_version, seeded.contract_version);
    assert_eq!(seeded_after_init.seed_locale, seeded.seed_locale);
    assert_eq!(seeded_after_init.seed_profile, seeded.seed_profile);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_init_creates_one_state_and_one_bootstrap_event() {
    let temp = TempDir::new().expect("temporary module root");
    write_module(temp.path(), "1.0.0", false);
    let database_path = temp.path().join("concurrent-init.sqlite");
    let pool = sqlite_pool(format!("sqlite:{}", database_path.display()), 2).await;
    let events = Arc::new(Mutex::new(Vec::new()));
    let first_module = Arc::new(
        ListeningModule::from_app_root(temp.path(), Arc::clone(&events))
            .expect("load first module"),
    );
    let second_module = Arc::new(
        ListeningModule::from_app_root(temp.path(), Arc::clone(&events))
            .expect("load second module"),
    );
    let first = LifecycleOrchestrator::new(pool.clone(), first_module);
    let second = LifecycleOrchestrator::new(pool.clone(), second_module);

    let (first_result, second_result) = tokio::join!(first.init(), second.init());
    first_result.expect("first concurrent init");
    second_result.expect("second concurrent init");
    let row_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM ops_database_installation_state WHERE module_id = 'state_machine'",
    )
    .fetch_one(pool.as_sqlite().expect("SQLite pool"))
    .await
    .expect("count installation state rows");
    assert_eq!(row_count, 1);
    assert_eq!(events.lock().expect("listener events should lock").len(), 1);
}
