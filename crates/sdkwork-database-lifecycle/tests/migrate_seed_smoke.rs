use std::sync::Arc;

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::{DefaultDatabaseModule, LocaleTag, SeedProfile};
use sdkwork_database_sqlx::create_pool_from_config;
use tempfile::TempDir;

fn write_file(path: &std::path::Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[tokio::test]
async fn migrate_and_seed_smoke() {
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
        &root.join("database/migrations/sqlite/0001_create_probe.up.sql"),
        "CREATE TABLE demo_probe (id INTEGER PRIMARY KEY, label TEXT NOT NULL);",
    );

    write_file(
        &root.join("database/seeds/seed.manifest.json"),
        r#"{
  "schemaVersion": 1,
  "kind": "sdkwork.database.seed",
  "defaultLocale": "zh-CN",
  "profiles": {
    "standard": {
      "common": ["001_probe.sql"],
      "locales": { "zh-CN": [] }
    }
  }
}"#,
    );

    write_file(
        &root.join("database/seeds/common/001_probe.sql"),
        "INSERT OR IGNORE INTO demo_probe (id, label) VALUES (1, 'zh-CN');",
    );

    let module = Arc::new(DefaultDatabaseModule::from_app_root(root).unwrap());
    let config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        ..Default::default()
    };
    let pool = create_pool_from_config(config).await.unwrap();
    let orchestrator = LifecycleOrchestrator::new(pool, module);

    let migrations = orchestrator.migrate().await.unwrap();
    assert_eq!(migrations, 1);

    let seeds = orchestrator
        .seed(&LocaleTag::zh_cn(), &SeedProfile::standard())
        .await
        .unwrap();
    assert_eq!(seeds, 1);

    let seeds_again = orchestrator
        .seed(&LocaleTag::zh_cn(), &SeedProfile::standard())
        .await
        .unwrap();
    assert_eq!(seeds_again, 0);
}
