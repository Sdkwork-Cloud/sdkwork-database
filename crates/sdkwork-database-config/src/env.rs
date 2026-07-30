use std::env;

use crate::database::{DatabaseConfig, DatabaseEngine, DeploymentMode};
use crate::error::ConfigError;
use crate::postgres::{PgSslMode, PostgresConfig};
use crate::sqlite::SqliteConfig;
use crate::workspace_database::{
    normalize_workspace_postgres_url, reject_retired_database_env, resolve_workspace_database_url,
};

/// Load one module's database configuration from the workspace-scoped environment.
///
/// `service_name` identifies table ownership only. Connection identity, schema,
/// pool sizing, and runtime policy always use `SDKWORK_DATABASE_*`.
pub fn load_from_env(service_name: &str) -> Result<DatabaseConfig, ConfigError> {
    reject_retired_database_env()?;
    let raw_url = resolve_workspace_database_url()?;
    let detected_engine = DatabaseEngine::from_url(&raw_url).ok_or_else(|| {
        ConfigError::InvalidUrl(format!("cannot detect database engine from URL: {raw_url}"))
    })?;
    let engine = match get_env_optional("SDKWORK_DATABASE_ENGINE") {
        Some(value) => {
            let configured = parse_database_engine(&value)?;
            if configured != detected_engine {
                return Err(ConfigError::InvalidConfig(format!(
                    "SDKWORK_DATABASE_ENGINE={value:?} conflicts with database URL engine {detected_engine}"
                )));
            }
            configured
        }
        None => detected_engine,
    };

    let url = match engine {
        DatabaseEngine::Postgres => normalize_workspace_postgres_url(&raw_url)?,
        DatabaseEngine::Sqlite => raw_url,
    };
    let mode = match engine {
        DatabaseEngine::Postgres => DeploymentMode::Integrated,
        DatabaseEngine::Sqlite => DeploymentMode::Standalone,
    };
    let table_prefix = match mode {
        DeploymentMode::Integrated => format!("{}_", service_name.to_ascii_lowercase()),
        DeploymentMode::Standalone => String::new(),
    };
    let max_connections = get_env_as("SDKWORK_DATABASE_MAX_CONNECTIONS", 10_u32)?;
    let min_connections = get_env_as("SDKWORK_DATABASE_MIN_CONNECTIONS", 1_u32)?;
    if min_connections > max_connections {
        return Err(ConfigError::InvalidConfig(format!(
            "SDKWORK_DATABASE_MIN_CONNECTIONS ({min_connections}) exceeds SDKWORK_DATABASE_MAX_CONNECTIONS ({max_connections})"
        )));
    }
    let postgres_ssl_mode = resolve_postgres_ssl_mode(&url);

    Ok(DatabaseConfig {
        engine,
        url,
        mode,
        table_prefix,
        max_connections,
        min_connections,
        acquire_timeout_secs: get_env_as("SDKWORK_DATABASE_ACQUIRE_TIMEOUT", 10_u64)?,
        idle_timeout_secs: get_env_as("SDKWORK_DATABASE_IDLE_TIMEOUT", 300_u64)?,
        max_lifetime_secs: get_env_as("SDKWORK_DATABASE_MAX_LIFETIME", 1800_u64)?,
        sqlite: SqliteConfig::default(),
        postgres: PostgresConfig {
            ssl_mode: postgres_ssl_mode,
            ..Default::default()
        },
    })
}

fn parse_database_engine(value: &str) -> Result<DatabaseEngine, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "sqlite" => Ok(DatabaseEngine::Sqlite),
        "postgres" | "postgresql" => Ok(DatabaseEngine::Postgres),
        _ => Err(ConfigError::InvalidEnvValue {
            key: "SDKWORK_DATABASE_ENGINE".to_string(),
            message: format!("unsupported database engine: {value}"),
        }),
    }
}

fn resolve_postgres_ssl_mode(url: &str) -> PgSslMode {
    get_env_optional("SDKWORK_DATABASE_SSL_MODE")
        .map(|value| parse_pg_ssl_mode(&value))
        .or_else(|| parse_pg_ssl_mode_from_url(url))
        .unwrap_or(PgSslMode::Prefer)
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
    url::Url::parse(url)
        .ok()?
        .query_pairs()
        .find_map(|(key, value)| {
            key.eq_ignore_ascii_case("sslmode")
                .then(|| parse_pg_ssl_mode(&value))
        })
}

fn get_env_optional(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn get_env_as<T: std::str::FromStr>(key: &str, default: T) -> Result<T, ConfigError> {
    match get_env_optional(key) {
        Some(value) => value
            .parse::<T>()
            .map_err(|_| ConfigError::InvalidEnvValue {
                key: key.to_string(),
                message: format!("cannot parse {value:?} as {}", std::any::type_name::<T>()),
            }),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    struct EnvGuard(Vec<(String, Option<String>)>);

    impl EnvGuard {
        fn set(values: &[(&str, Option<&str>)]) -> Self {
            let previous = values
                .iter()
                .map(|(key, _)| ((*key).to_string(), env::var(*key).ok()))
                .collect();
            for (key, value) in values {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
            Self(previous)
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }

    fn clear_database_env() -> EnvGuard {
        EnvGuard::set(&[
            ("SDKWORK_DATABASE_URL", None),
            ("SDKWORK_DATABASE_ENGINE", None),
            ("SDKWORK_DATABASE_HOST", None),
            ("SDKWORK_DATABASE_PORT", None),
            ("SDKWORK_DATABASE_NAME", None),
            ("SDKWORK_DATABASE_SCHEMA", None),
            ("SDKWORK_DATABASE_USERNAME", None),
            ("SDKWORK_DATABASE_PASSWORD", None),
            ("SDKWORK_DATABASE_PASSWORD_FILE", None),
            ("SDKWORK_DATABASE_SSL_MODE", None),
            ("SDKWORK_DATABASE_FILE", None),
            ("SDKWORK_DATABASE_MAX_CONNECTIONS", None),
            ("SDKWORK_DATABASE_MIN_CONNECTIONS", None),
            ("SDKWORK_DATABASE_ACQUIRE_TIMEOUT", None),
            ("SDKWORK_DATABASE_IDLE_TIMEOUT", None),
            ("SDKWORK_DATABASE_MAX_LIFETIME", None),
            ("DATABASE_URL", None),
        ])
    }

    #[test]
    #[serial]
    fn unset_environment_uses_workspace_development_profile() {
        let _guard = clear_database_env();
        let config = load_from_env("MODELS").unwrap();
        assert_eq!(config.engine, DatabaseEngine::Postgres);
        assert_eq!(config.mode, DeploymentMode::Integrated);
        assert_eq!(config.table_prefix, "models_");
        assert!(config.url.contains("/sdkwork_ai_dev"));
        assert!(config.url.contains("search_path%3Dsdkwork_ai_dev%2Cpublic"));
    }

    #[test]
    #[serial]
    fn sqlite_uses_generic_client_local_keys() {
        let _cleared = clear_database_env();
        let _configured = EnvGuard::set(&[
            ("SDKWORK_DATABASE_ENGINE", Some("sqlite")),
            ("SDKWORK_DATABASE_FILE", Some("test.db")),
        ]);
        let config = load_from_env("MODELS").unwrap();
        assert_eq!(config.engine, DatabaseEngine::Sqlite);
        assert_eq!(config.url, "sqlite:test.db");
        assert_eq!(config.mode, DeploymentMode::Standalone);
        assert!(config.table_prefix.is_empty());
    }

    #[test]
    #[serial]
    fn generic_pool_settings_apply_to_every_module() {
        let _cleared = clear_database_env();
        let _configured = EnvGuard::set(&[
            ("SDKWORK_DATABASE_MAX_CONNECTIONS", Some("32")),
            ("SDKWORK_DATABASE_MIN_CONNECTIONS", Some("4")),
        ]);
        let models = load_from_env("MODELS").unwrap();
        let iam = load_from_env("IAM").unwrap();
        assert_eq!(models.max_connections, 32);
        assert_eq!(iam.max_connections, 32);
        assert_eq!(models.min_connections, 4);
        assert_eq!(iam.min_connections, 4);
        assert_eq!(models.url, iam.url);
    }

    #[test]
    #[serial]
    fn service_prefixed_database_key_fails_closed() {
        let _cleared = clear_database_env();
        let retired_key = ["SDKWORK", "MODELS", "DATABASE", "URL"].join("_");
        let previous = env::var(&retired_key).ok();
        env::set_var(&retired_key, "postgresql://models:secret@localhost/models");
        let error = load_from_env("MODELS").unwrap_err().to_string();
        match previous {
            Some(value) => env::set_var(&retired_key, value),
            None => env::remove_var(&retired_key),
        }
        assert!(error.contains(&retired_key));
    }

    #[test]
    #[serial]
    fn conflicting_engine_and_url_fail_closed() {
        let _cleared = clear_database_env();
        let _configured = EnvGuard::set(&[
            ("SDKWORK_DATABASE_ENGINE", Some("sqlite")),
            (
                "SDKWORK_DATABASE_URL",
                Some("postgresql://sdkwork_ai_dev:secret@localhost/sdkwork_ai_dev"),
            ),
        ]);
        let error = load_from_env("MODELS").unwrap_err().to_string();
        assert!(error.contains("conflicts with database URL engine"));
    }
}
