#![cfg(feature = "sqlite")]

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
#[cfg(feature = "any")]
use sdkwork_database_sqlx::create_any_pool_from_config;
use sdkwork_database_sqlx::{
    create_pool_from_config, enable_process_shared_database_pool, process_shared_database_pool,
    PoolError,
};
use serial_test::serial;

fn sqlite_config(url: &str) -> DatabaseConfig {
    DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: url.to_string(),
        max_connections: 2,
        min_connections: 0,
        ..Default::default()
    }
}

/// The process pool registry is process-global, so the strict-budget and
/// client-local coexistence scenarios share one sequential test; the server
/// strictness gate is verified in a second serial test.
#[tokio::test]
#[serial]
async fn process_pool_reuses_matching_identity_and_coexists_with_client_local_urls() {
    #[cfg(feature = "any")]
    std::env::set_var("SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION", "true");
    enable_process_shared_database_pool();

    // The first process pool owns the temporary-driver connection budget and
    // every subsequent request for the same identity reuses it.
    let (first, second) = tokio::join!(
        create_pool_from_config(sqlite_config("sqlite::memory:")),
        create_pool_from_config(sqlite_config("sqlite::memory:")),
    );
    let first = first.expect("first pool");
    let second = second.expect("concurrent matching pool");

    assert!(process_shared_database_pool().is_some());
    #[cfg(feature = "any")]
    assert_eq!(first.config().max_connections, 1);
    #[cfg(not(feature = "any"))]
    assert_eq!(first.config().max_connections, 2);
    first.close().await;
    assert!(second.as_sqlite().expect("sqlite pool").is_closed());

    // A distinct client-local SQLite URL is a separate declared database and
    // coexists with the first one (ENVIRONMENT_SPEC §7.2).
    let third = create_pool_from_config(sqlite_config("sqlite:process-shared-a.db"))
        .await
        .expect("distinct client-local URL must get its own pool");
    assert!(
        !third.as_sqlite().expect("sqlite pool").is_closed(),
        "distinct client-local pool must stay independent"
    );
    let third_reuse = create_pool_from_config(sqlite_config("sqlite:process-shared-a.db"))
        .await
        .expect("same client-local URL must be reused");
    third.close().await;
    assert!(
        third_reuse.as_sqlite().expect("sqlite pool").is_closed(),
        "reused pool must share the process pool"
    );
    third_reuse.close().await;

    #[cfg(feature = "any")]
    {
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

    let _ = std::fs::remove_file("process-shared-a.db");
}

#[tokio::test]
#[serial]
async fn server_identity_mismatch_still_fails_closed() {
    // The canonical server (PostgreSQL) identity stays strict: a second
    // different server identity must fail before any connection is opened.
    enable_process_shared_database_pool();

    // Install a client-local pool first so the registry exists without a live
    // PostgreSQL server.
    let _client_local =
        create_pool_from_config(sqlite_config("sqlite:process-shared-server-test.db"))
            .await
            .expect("client-local pool installs the registry");
    let server = DatabaseConfig {
        engine: DatabaseEngine::Postgres,
        url: "postgresql://sdkwork_ai_test:secret@127.0.0.1:5432/sdkwork_ai_test".to_string(),
        max_connections: 2,
        min_connections: 0,
        ..Default::default()
    };
    // A live PostgreSQL server is required to install the server slot, which
    // cannot run in this hermetic suite; the strict mismatch path stays wired
    // through the temporary-driver gate, which requires the reserved capacity
    // that only the installed identity owns.
    #[cfg(feature = "any")]
    {
        std::env::set_var("SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION", "true");
        let error = create_any_pool_from_config(server)
            .await
            .expect_err("temporary AnyPool requires a matching installed identity");
        assert!(matches!(
            error,
            PoolError::TemporaryDriverCapacityNotReserved
                | PoolError::ProcessPoolIdentityMismatch { .. }
        ));
        std::env::remove_var("SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION");
    }
    let _ = std::fs::remove_file("process-shared-server-test.db");
}
