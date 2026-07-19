use std::sync::atomic::{AtomicBool, Ordering};

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine, PgSslMode};
use sqlx::AnyPool;
use tokio::sync::OnceCell;
use url::Url;

use crate::{DatabasePool, PoolBuilder, PoolError};

const DATABASE_POOL_DRIVER: &str = "sqlx::DatabasePool";

static PROCESS_POOL_ENABLED: AtomicBool = AtomicBool::new(false);
static PROCESS_POOL: OnceCell<ProcessPoolEntry> = OnceCell::const_new();
static TEMPORARY_ANY_POOL: OnceCell<TemporaryAnyPoolEntry> = OnceCell::const_new();

struct ProcessPoolEntry {
    identity: DatabaseIdentity,
    pool: DatabasePool,
    temporary_driver_pool_count: u32,
    temporary_driver_max_connections: u32,
}

struct TemporaryAnyPoolEntry {
    identity: DatabaseIdentity,
    pool: AnyPool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DatabaseIdentity {
    engine: DatabaseEngine,
    driver: &'static str,
    endpoint: String,
    database: String,
    schema: String,
    credential_identity: String,
    tls_mode: String,
}

impl DatabaseIdentity {
    fn from_config(config: &DatabaseConfig) -> Result<Self, PoolError> {
        match config.engine {
            DatabaseEngine::Postgres => Self::from_postgres_config(config),
            DatabaseEngine::Sqlite => Self::from_sqlite_config(config),
        }
    }

    fn from_postgres_config(config: &DatabaseConfig) -> Result<Self, PoolError> {
        let url = Url::parse(&config.url)
            .map_err(|error| PoolError::InvalidUrl(format!("PostgreSQL URL: {error}")))?;
        let host = url.host_str().unwrap_or("localhost").to_ascii_lowercase();
        let port = url.port().unwrap_or(5432);
        let database = url.path().trim_start_matches('/').to_string();
        let mut schema = "public".to_string();
        let mut tls_mode = postgres_ssl_mode_name(config.postgres.ssl_mode).to_string();

        for (key, value) in url.query_pairs() {
            if key.eq_ignore_ascii_case("sslmode") {
                tls_mode = value.to_ascii_lowercase();
            } else if key.eq_ignore_ascii_case("options") {
                if let Some(value) = option_value(&value, "search_path") {
                    schema = value;
                }
            }
        }

        Ok(Self {
            engine: DatabaseEngine::Postgres,
            driver: DATABASE_POOL_DRIVER,
            endpoint: format!("{host}:{port}"),
            database,
            schema,
            credential_identity: url.username().to_string(),
            tls_mode,
        })
    }

    fn from_sqlite_config(config: &DatabaseConfig) -> Result<Self, PoolError> {
        if config.url.is_empty() {
            return Err(PoolError::InvalidUrl(
                "SQLite URL must not be empty".to_string(),
            ));
        }

        let database = config
            .url
            .split_once('?')
            .map_or(config.url.as_str(), |(path, _)| path)
            .replace('\\', "/");

        Ok(Self {
            engine: DatabaseEngine::Sqlite,
            driver: DATABASE_POOL_DRIVER,
            endpoint: "local".to_string(),
            database,
            schema: "main".to_string(),
            credential_identity: "process".to_string(),
            tls_mode: "none".to_string(),
        })
    }

    fn redacted(&self) -> String {
        format!(
            "engine={}, driver={}, endpoint={}, database={}, schema={}, credential_identity={}, tls={}",
            self.engine,
            self.driver,
            self.endpoint,
            self.database,
            self.schema,
            self.credential_identity,
            self.tls_mode
        )
    }
}

/// Enable strict process-local pool reuse for all subsequent pool creation calls.
///
/// The process entrypoint must call this before any database lifecycle bootstrap.
pub fn enable_process_shared_database_pool() {
    PROCESS_POOL_ENABLED.store(true, Ordering::Release);
}

/// Return whether strict process-local pool reuse has been enabled.
pub fn process_shared_database_pool_enabled() -> bool {
    PROCESS_POOL_ENABLED.load(Ordering::Acquire)
}

/// Return a clone of the installed process pool, if the first pool has been created.
pub fn process_shared_database_pool() -> Option<DatabasePool> {
    PROCESS_POOL.get().map(|entry| entry.pool.clone())
}

/// Return the per-pool capacity reserved for each declared temporary driver.
pub fn process_shared_temporary_driver_max_connections() -> Option<u32> {
    PROCESS_POOL
        .get()
        .filter(|entry| entry.temporary_driver_pool_count > 0)
        .map(|entry| entry.temporary_driver_max_connections)
}

pub(crate) async fn create_or_reuse_process_pool(
    config: DatabaseConfig,
) -> Result<DatabasePool, PoolError> {
    let config = normalize_config_engine(config)?;
    if !process_shared_database_pool_enabled() {
        return PoolBuilder::new(config).build_unshared().await;
    }

    let requested_identity = DatabaseIdentity::from_config(&config)?;
    let initialization_identity = requested_identity.clone();
    let process_max_connections = config.max_connections;
    let temporary_driver_pool_count = configured_temporary_driver_pool_count()?;
    let (canonical_max_connections, temporary_driver_max_connections) =
        temporary_driver_connection_budget(process_max_connections, temporary_driver_pool_count)?;
    let entry = PROCESS_POOL
        .get_or_try_init(|| async move {
            let canonical_config = config_with_max_connections(config, canonical_max_connections);
            let pool = PoolBuilder::new(canonical_config).build_unshared().await?;
            Ok::<_, PoolError>(ProcessPoolEntry {
                identity: initialization_identity,
                pool,
                temporary_driver_pool_count,
                temporary_driver_max_connections,
            })
        })
        .await?;

    if entry.identity != requested_identity {
        return Err(PoolError::ProcessPoolIdentityMismatch {
            installed: entry.identity.redacted(),
            requested: requested_identity.redacted(),
        });
    }

    Ok(entry.pool.clone())
}

pub(crate) async fn create_or_reuse_temporary_any_pool(
    config: DatabaseConfig,
) -> Result<AnyPool, PoolError> {
    if !temporary_any_pool_exception_enabled() {
        return Err(PoolError::ProcessPoolDriverMismatch {
            installed: DATABASE_POOL_DRIVER,
            requested: "sqlx::AnyPool",
        });
    }

    let config = normalize_config_engine(config)?;
    let requested_identity = DatabaseIdentity::from_config(&config)?;
    let process_entry = PROCESS_POOL
        .get()
        .ok_or(PoolError::ProcessPoolNotInstalled)?;
    if process_entry.temporary_driver_pool_count == 0 {
        return Err(PoolError::TemporaryDriverCapacityNotReserved);
    }
    if process_entry.identity != requested_identity {
        return Err(PoolError::ProcessPoolIdentityMismatch {
            installed: process_entry.identity.redacted(),
            requested: requested_identity.redacted(),
        });
    }

    let initialization_identity = requested_identity.clone();
    let config =
        config_with_max_connections(config, process_entry.temporary_driver_max_connections);
    let entry = TEMPORARY_ANY_POOL
        .get_or_try_init(|| async move {
            let pool = crate::any::create_any_pool(&config).await?;
            Ok::<_, PoolError>(TemporaryAnyPoolEntry {
                identity: initialization_identity,
                pool,
            })
        })
        .await?;

    if entry.identity != requested_identity {
        return Err(PoolError::ProcessPoolIdentityMismatch {
            installed: entry.identity.redacted(),
            requested: requested_identity.redacted(),
        });
    }

    Ok(entry.pool.clone())
}

pub(crate) fn redacted_identity(config: &DatabaseConfig) -> String {
    normalize_config_engine(config.clone())
        .and_then(|config| DatabaseIdentity::from_config(&config))
        .map_or_else(
            |_| "invalid database identity".to_string(),
            |identity| identity.redacted(),
        )
}

fn temporary_any_pool_exception_enabled() -> bool {
    std::env::var("SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION")
        .is_ok_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true"))
}

fn configured_temporary_driver_pool_count() -> Result<u32, PoolError> {
    let raw = std::env::var("SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT").unwrap_or_default();
    if raw.trim().is_empty() {
        return Ok(u32::from(temporary_any_pool_exception_enabled()));
    }
    let count = raw.trim().parse::<u32>().map_err(|error| {
        PoolError::DatabaseConfig(format!(
            "SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT must be a non-negative integer: {error}"
        ))
    })?;
    if temporary_any_pool_exception_enabled() && count == 0 {
        return Err(PoolError::DatabaseConfig(
            "SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT must be at least 1 while the temporary AnyPool exception is enabled"
                .to_string(),
        ));
    }
    Ok(count)
}

fn temporary_driver_connection_budget(
    process_max: u32,
    temporary_driver_count: u32,
) -> Result<(u32, u32), PoolError> {
    if temporary_driver_count == 0 {
        return Ok((process_max, 0));
    }
    if process_max <= temporary_driver_count {
        return Err(PoolError::DatabaseConfig(
            format!(
                "process max_connections must be at least {} while {temporary_driver_count} temporary driver pool(s) are declared",
                temporary_driver_count + 1
            ),
        ));
    }
    let temporary_max = process_max / (temporary_driver_count + 1);
    let canonical_max = process_max - temporary_max * temporary_driver_count;
    Ok((canonical_max, temporary_max))
}

fn config_with_max_connections(mut config: DatabaseConfig, max_connections: u32) -> DatabaseConfig {
    config.max_connections = max_connections;
    config.min_connections = config.min_connections.min(max_connections);
    config
}

fn normalize_config_engine(mut config: DatabaseConfig) -> Result<DatabaseConfig, PoolError> {
    if config.url.is_empty() {
        return Err(PoolError::InvalidUrl(
            "Database URL is empty. Set SDKWORK_*_DATABASE_URL environment variable.".to_string(),
        ));
    }

    if let Some(engine) = DatabaseEngine::from_url(&config.url) {
        config.engine = engine;
    }

    Ok(config)
}

fn option_value(options: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=");
    let start = options.find(&marker)? + marker.len();
    let value = options[start..]
        .split_ascii_whitespace()
        .next()?
        .trim_matches(['\'', '"']);
    (!value.is_empty()).then(|| value.to_string())
}

fn postgres_ssl_mode_name(mode: PgSslMode) -> &'static str {
    match mode {
        PgSslMode::Disable => "disable",
        PgSslMode::Allow => "allow",
        PgSslMode::Prefer => "prefer",
        PgSslMode::Require => "require",
        PgSslMode::VerifyCa => "verify_ca",
        PgSslMode::VerifyFull => "verify_full",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn postgres_config(url: &str) -> DatabaseConfig {
        DatabaseConfig {
            engine: DatabaseEngine::Postgres,
            url: url.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn postgres_identity_ignores_password_and_query_order() {
        let first = DatabaseIdentity::from_config(&postgres_config(
            "postgresql://app:first@DB.INTERNAL:5432/app?sslmode=disable&options=-c%20search_path%3Dsdkwork_ai_dev%2Cpublic",
        ))
        .unwrap();
        let second = DatabaseIdentity::from_config(&postgres_config(
            "postgres://app:second@db.internal/app?options=-c+search_path%3Dsdkwork_ai_dev%2Cpublic&sslmode=disable",
        ))
        .unwrap();

        assert_eq!(first, second);
        assert!(!first.redacted().contains("first"));
        assert!(!first.redacted().contains("second"));
    }

    #[test]
    fn postgres_identity_detects_schema_mismatch() {
        let first = DatabaseIdentity::from_config(&postgres_config(
            "postgresql://app@localhost/app?options=-c%20search_path%3Done",
        ))
        .unwrap();
        let second = DatabaseIdentity::from_config(&postgres_config(
            "postgresql://app@localhost/app?options=-c%20search_path%3Dtwo",
        ))
        .unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn temporary_driver_budget_is_bounded_and_favors_canonical_pool() {
        assert_eq!(temporary_driver_connection_budget(10, 0).unwrap(), (10, 0));
        assert_eq!(temporary_driver_connection_budget(10, 1).unwrap(), (5, 5));
        assert_eq!(temporary_driver_connection_budget(11, 1).unwrap(), (6, 5));
        assert_eq!(temporary_driver_connection_budget(10, 2).unwrap(), (4, 3));
        assert_eq!(temporary_driver_connection_budget(2, 1).unwrap(), (1, 1));
        assert!(temporary_driver_connection_budget(2, 2).is_err());
    }
}
