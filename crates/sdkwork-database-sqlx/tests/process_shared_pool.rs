use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_sqlx::{
    create_any_pool_from_config, create_pool_from_config, enable_process_shared_database_pool,
    process_shared_database_pool, PoolError,
};

fn sqlite_config(url: &str) -> DatabaseConfig {
    DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: url.to_string(),
        max_connections: 2,
        min_connections: 0,
        ..Default::default()
    }
}

#[tokio::test]
async fn strict_process_pool_reuses_matching_identity_and_rejects_mismatches() {
    std::env::set_var("SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION", "true");
    enable_process_shared_database_pool();

    let (first, second) = tokio::join!(
        create_pool_from_config(sqlite_config("sqlite::memory:")),
        create_pool_from_config(sqlite_config("sqlite::memory:")),
    );
    let first = first.expect("first pool");
    let second = second.expect("concurrent matching pool");

    assert!(process_shared_database_pool().is_some());
    assert_eq!(first.config().max_connections, 1);
    first.close().await;
    assert!(second.as_sqlite().expect("sqlite pool").is_closed());

    let mismatch = create_pool_from_config(sqlite_config("sqlite://other.db"))
        .await
        .expect_err("different identity must fail");
    assert!(matches!(
        mismatch,
        PoolError::ProcessPoolIdentityMismatch { .. }
    ));

    let temporary = create_any_pool_from_config(sqlite_config("sqlite::memory:"))
        .await
        .expect("declared temporary AnyPool exception");
    assert_eq!(temporary.options().get_max_connections(), 1);
    let temporary_clone = create_any_pool_from_config(sqlite_config("sqlite::memory:"))
        .await
        .expect("temporary AnyPool must be reused");
    temporary.close().await;
    assert!(temporary_clone.is_closed());
    std::env::remove_var("SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION");
}
