use std::env;

use crate::database::{DatabaseConfig, DatabaseEngine, DeploymentMode};
use crate::error::ConfigError;
use crate::postgres::{PgSslMode, PostgresConfig};
use crate::sqlite::SqliteConfig;

/// Load database configuration from environment variables.
///
/// Environment variable naming convention:
/// - `SDKWORK_{SERVICE}_DATABASE_URL` - database connection URL
/// - `SDKWORK_{SERVICE}_DATABASE_ENGINE` - database engine (sqlite/postgres)
/// - `SDKWORK_{SERVICE}_DATABASE_MODE` - deployment mode (standalone/integrated)
/// - `SDKWORK_{SERVICE}_DATABASE_TABLE_PREFIX` - table prefix for integrated mode
/// - `SDKWORK_{SERVICE}_DATABASE_MAX_CONNECTIONS` - max connections
/// - `SDKWORK_{SERVICE}_DATABASE_MIN_CONNECTIONS` - min connections
/// - `SDKWORK_{SERVICE}_DATABASE_ACQUIRE_TIMEOUT` - acquire timeout (seconds)
/// - `SDKWORK_{SERVICE}_DATABASE_IDLE_TIMEOUT` - idle timeout (seconds)
/// - `SDKWORK_{SERVICE}_DATABASE_MAX_LIFETIME` - max lifetime (seconds)
///
/// Falls back to the unified sdkwork-clawrouter database profile:
/// - `SDKWORK_DATABASE_URL`
/// - `DATABASE_URL` (legacy)
/// - `SDKWORK_CLAW_DATABASE_URL`
/// - `SDKWORK_CLAW_DATABASE_ENGINE/HOST/PORT/NAME/USERNAME/PASSWORD/SSL_MODE`
/// - default local PostgreSQL development database (`sdkwork_ai_dev` on `127.0.0.1:5432`)
pub fn load_from_env(service_name: &str) -> Result<DatabaseConfig, ConfigError> {
    let prefix = format!("SDKWORK_{}", service_name.to_uppercase());

    let url = crate::claw_database::resolve_unified_database_url(&prefix)?;

    // Detect or load engine
    let engine = get_env_optional(&format!("{prefix}_DATABASE_ENGINE"))
        .or_else(|| get_env_optional("SDKWORK_CLAW_DATABASE_ENGINE"))
        .and_then(|v| match v.to_lowercase().as_str() {
            "sqlite" => Some(DatabaseEngine::Sqlite),
            "postgres" | "postgresql" => Some(DatabaseEngine::Postgres),
            _ => None,
        })
        .or_else(|| DatabaseEngine::from_url(&url))
        .ok_or_else(|| ConfigError::InvalidUrl(format!("Cannot detect engine from URL: {url}")))?;

    let url = if engine == DatabaseEngine::Postgres {
        crate::claw_database::postgres_url_with_search_path(&url, &prefix)
    } else {
        url
    };

    // Load deployment mode
    let mode = get_env_optional(&format!("{prefix}_DATABASE_MODE"))
        .and_then(|v| match v.to_lowercase().as_str() {
            "standalone" => Some(DeploymentMode::Standalone),
            "integrated" => Some(DeploymentMode::Integrated),
            _ => None,
        })
        .unwrap_or_default();

    // Load table prefix
    let table_prefix = get_env_optional(&format!("{prefix}_DATABASE_TABLE_PREFIX")).unwrap_or_else(
        || match mode {
            DeploymentMode::Standalone => String::new(),
            DeploymentMode::Integrated => format!("{}_", service_name.to_lowercase()),
        },
    );

    let max_connections = match get_env_optional(&format!("{prefix}_DATABASE_MAX_CONNECTIONS"))
        .or_else(|| get_env_optional("SDKWORK_CLAW_DATABASE_MAX_CONNECTIONS"))
    {
        Some(value) => value
            .parse::<u32>()
            .map_err(|_| ConfigError::InvalidEnvValue {
                key: format!("{prefix}_DATABASE_MAX_CONNECTIONS"),
                message: format!("Cannot parse '{value}' as u32"),
            })?,
        None => crate::claw_database::resolve_unified_max_connections(&prefix),
    };
    let min_connections = get_env_as::<u32>(&format!("{}_DATABASE_MIN_CONNECTIONS", prefix), 1)?;
    let acquire_timeout_secs =
        get_env_as::<u64>(&format!("{}_DATABASE_ACQUIRE_TIMEOUT", prefix), 10)?;
    let idle_timeout_secs = get_env_as::<u64>(&format!("{}_DATABASE_IDLE_TIMEOUT", prefix), 300)?;
    let max_lifetime_secs = get_env_as::<u64>(&format!("{}_DATABASE_MAX_LIFETIME", prefix), 1800)?;

    let mut postgres = PostgresConfig::default();
    postgres.ssl_mode = resolve_postgres_ssl_mode(&prefix, &url);

    Ok(DatabaseConfig {
        engine,
        url,
        mode,
        table_prefix,
        max_connections,
        min_connections,
        acquire_timeout_secs,
        idle_timeout_secs,
        max_lifetime_secs,
        sqlite: SqliteConfig::default(),
        postgres,
    })
}

fn resolve_postgres_ssl_mode(service_prefix: &str, url: &str) -> PgSslMode {
    for key in [
        format!("{service_prefix}_DATABASE_SSL_MODE"),
        "SDKWORK_CLAW_DATABASE_SSL_MODE".to_string(),
    ] {
        if let Some(value) = get_env_optional(&key) {
            return parse_pg_ssl_mode(&value);
        }
    }
    if let Some(mode) = parse_pg_ssl_mode_from_url(url) {
        return mode;
    }
    PgSslMode::Prefer
}

fn parse_pg_ssl_mode(value: &str) -> PgSslMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "disable" => PgSslMode::Disable,
        "allow" => PgSslMode::Allow,
        "prefer" => PgSslMode::Prefer,
        "require" => PgSslMode::Require,
        "verify-ca" | "verify_ca" => PgSslMode::VerifyCa,
        "verify-full" | "verify_full" => PgSslMode::VerifyFull,
        _ => PgSslMode::Prefer,
    }
}

fn parse_pg_ssl_mode_from_url(url: &str) -> Option<PgSslMode> {
    let query = url.split('?').nth(1)?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key.eq_ignore_ascii_case("sslmode") {
            return Some(parse_pg_ssl_mode(value));
        }
    }
    None
}

fn get_env_optional(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.is_empty())
}

fn get_env_as<T: std::str::FromStr>(key: &str, default: T) -> Result<T, ConfigError> {
    match get_env_optional(key) {
        Some(value) => value
            .parse::<T>()
            .map_err(|_| ConfigError::InvalidEnvValue {
                key: key.to_string(),
                message: format!("Cannot parse '{}' as {}", value, std::any::type_name::<T>()),
            }),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::{LazyLock, Mutex};

    static ENV_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn lock_env_tests() -> std::sync::MutexGuard<'static, ()> {
        ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    struct EnvGuard {
        previous: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn set(values: &[(&str, Option<&str>)]) -> Self {
            let previous = values
                .iter()
                .map(|(key, _)| ((*key).to_string(), env::var(*key).ok()))
                .collect::<Vec<_>>();
            for (key, value) in values {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
            Self { previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.previous {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn test_load_from_env_uses_claw_router_default_when_unset() {
        let _lock = lock_env_tests();
        let _guard = EnvGuard::set(&[
            ("SDKWORK_MISSING_TEST_DATABASE_URL", None),
            ("SDKWORK_DATABASE_URL", None),
            ("DATABASE_URL", None),
            ("SDKWORK_CLAW_DATABASE_URL", None),
            ("SDKWORK_CLAW_DATABASE_ENGINE", None),
            ("SDKWORK_CLAW_DATABASE_HOST", None),
            ("SDKWORK_CLAW_DATABASE_PORT", None),
            ("SDKWORK_CLAW_DATABASE_NAME", None),
            ("SDKWORK_CLAW_DATABASE_USERNAME", None),
            ("SDKWORK_CLAW_DATABASE_PASSWORD", None),
            ("SDKWORK_CLAW_DATABASE_SSL_MODE", None),
        ]);

        let config = load_from_env("MISSING_TEST").expect("default claw profile should resolve");
        assert_eq!(config.engine, DatabaseEngine::Postgres);
        assert_eq!(
            config.url,
            crate::claw_database::default_claw_router_dev_postgres_database_url()
        );
    }

    #[test]
    fn test_load_from_env_sqlite() {
        let _lock = lock_env_tests();
        let _guard = EnvGuard::set(&[
            ("SDKWORK_SQLITE_TEST_DATABASE_URL", Some("sqlite:test.db")),
            ("SDKWORK_SQLITE_TEST_DATABASE_ENGINE", Some("sqlite")),
            ("SDKWORK_DATABASE_URL", None),
            ("DATABASE_URL", None),
            ("SDKWORK_CLAW_DATABASE_URL", None),
            ("SDKWORK_CLAW_DATABASE_ENGINE", None),
            ("SDKWORK_CLAW_DATABASE_HOST", None),
        ]);

        let config = load_from_env("SQLITE_TEST").unwrap();
        assert_eq!(config.engine, DatabaseEngine::Sqlite);
        assert_eq!(config.url, "sqlite:test.db");
        assert_eq!(config.max_connections, 10);
    }

    #[test]
    fn test_load_from_env_postgres() {
        let url_key = "SDKWORK_PG_TEST_DATABASE_URL";
        let max_key = "SDKWORK_PG_TEST_DATABASE_MAX_CONNECTIONS";
        env::set_var(url_key, "postgres://localhost/test");
        env::set_var(max_key, "32");

        let config = load_from_env("PG_TEST").unwrap();
        assert_eq!(config.engine, DatabaseEngine::Postgres);
        assert_eq!(config.max_connections, 32);

        env::remove_var(url_key);
        env::remove_var(max_key);
    }

    #[test]
    fn test_load_from_env_postgres_ssl_mode_from_env() {
        let _lock = lock_env_tests();
        let _guard = EnvGuard::set(&[
            ("SDKWORK_PG_SSL_TEST_DATABASE_URL", Some("postgresql://127.0.0.1/test")),
            ("SDKWORK_PG_SSL_TEST_DATABASE_SSL_MODE", Some("disable")),
        ]);

        let config = load_from_env("PG_SSL_TEST").unwrap();
        assert_eq!(config.postgres.ssl_mode, PgSslMode::Disable);

    }

    #[test]
    fn test_load_from_env_postgres_ssl_mode_from_url() {
        let _lock = lock_env_tests();
        let _guard = EnvGuard::set(&[(
            "SDKWORK_PG_SSL_URL_TEST_DATABASE_URL",
            Some("postgresql://127.0.0.1/test?sslmode=disable"),
        )]);

        let config = load_from_env("PG_SSL_URL_TEST").unwrap();
        assert_eq!(config.postgres.ssl_mode, PgSslMode::Disable);
    }

    #[test]
    fn test_load_from_env_integrated_mode() {
        let url_key = "SDKWORK_INT_TEST_DATABASE_URL";
        let mode_key = "SDKWORK_INT_TEST_DATABASE_MODE";
        env::set_var(url_key, "postgres://localhost/shared");
        env::set_var(mode_key, "integrated");

        let config = load_from_env("INT_TEST").unwrap();
        assert_eq!(config.mode, DeploymentMode::Integrated);
        assert_eq!(config.table_prefix, "int_test_");

        env::remove_var(url_key);
        env::remove_var(mode_key);
    }
}
