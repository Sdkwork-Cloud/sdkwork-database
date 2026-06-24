use crate::error::ConfigError;

const DEFAULT_DEV_POSTGRES_HOST: &str = "[::1]";
const DEFAULT_DEV_POSTGRES_PORT: &str = "5432";
const DEFAULT_DEV_POSTGRES_DATABASE: &str = "sdkwork_ai_dev";
const DEFAULT_DEV_POSTGRES_USERNAME: &str = "sdkwork_ai_dev";
const DEFAULT_DEV_POSTGRES_PASSWORD: &str = "sdkworkdev123";
const DEFAULT_DEV_POSTGRES_SSL_MODE: &str = "disable";
const DEFAULT_DEV_POSTGRES_MAX_CONNECTIONS: u32 = 10;

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_optional(key: &str) -> Option<String> {
    normalize_optional(std::env::var(key).ok())
}

fn percent_encode_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect::<String>()
}

fn percent_encode_path_segment(value: &str) -> String {
    percent_encode_component(value).replace("%2F", "/")
}

/// Build a PostgreSQL URL using the same field semantics as sdkwork-claw-router.
pub fn build_postgres_database_url(
    host: &str,
    port: Option<&str>,
    database: &str,
    username: &str,
    password: &str,
    ssl_mode: Option<&str>,
) -> String {
    let credentials = format!(
        "{}:{}",
        percent_encode_component(username),
        percent_encode_component(password)
    );
    let authority = match port.filter(|value| !value.is_empty()) {
        Some(port) => format!("{credentials}@{host}:{port}"),
        None => format!("{credentials}@{host}"),
    };
    let mut query = Vec::new();
    if let Some(ssl_mode) = ssl_mode.filter(|value| !value.is_empty()) {
        query.push(format!("sslmode={}", percent_encode_component(ssl_mode)));
    }
    match query.is_empty() {
        true => format!(
            "postgresql://{authority}/{}",
            percent_encode_path_segment(database)
        ),
        false => format!(
            "postgresql://{authority}/{}?{}",
            percent_encode_path_segment(database),
            query.join("&")
        ),
    }
}

/// Default local PostgreSQL development URL aligned with sdkwork-claw-router.
pub fn default_claw_router_dev_postgres_database_url() -> String {
    build_postgres_database_url(
        DEFAULT_DEV_POSTGRES_HOST,
        Some(DEFAULT_DEV_POSTGRES_PORT),
        DEFAULT_DEV_POSTGRES_DATABASE,
        DEFAULT_DEV_POSTGRES_USERNAME,
        DEFAULT_DEV_POSTGRES_PASSWORD,
        Some(DEFAULT_DEV_POSTGRES_SSL_MODE),
    )
}

pub fn default_claw_router_dev_postgres_max_connections() -> u32 {
    DEFAULT_DEV_POSTGRES_MAX_CONNECTIONS
}

fn resolve_postgres_database_url_from_split_fields() -> Result<Option<String>, ConfigError> {
    if env_optional("SDKWORK_CLAW_DATABASE_PROVIDER").is_some() {
        return Err(ConfigError::InvalidEnvValue {
            key: "SDKWORK_CLAW_DATABASE_PROVIDER".to_string(),
            message:
                "SDKWORK_CLAW_DATABASE_PROVIDER is not supported; use SDKWORK_CLAW_DATABASE_ENGINE"
                    .to_string(),
        });
    }
    if env_optional("SDKWORK_CLAW_DATABASE_SSLMODE").is_some() {
        return Err(ConfigError::InvalidEnvValue {
            key: "SDKWORK_CLAW_DATABASE_SSLMODE".to_string(),
            message:
                "SDKWORK_CLAW_DATABASE_SSLMODE is not supported; use SDKWORK_CLAW_DATABASE_SSL_MODE"
                    .to_string(),
        });
    }

    let Some(engine) = env_optional("SDKWORK_CLAW_DATABASE_ENGINE") else {
        return Ok(None);
    };
    if !matches!(
        engine.to_ascii_lowercase().as_str(),
        "postgres" | "postgresql"
    ) {
        return Err(ConfigError::InvalidEnvValue {
            key: "SDKWORK_CLAW_DATABASE_ENGINE".to_string(),
            message: format!("unsupported SDKWORK_CLAW_DATABASE_ENGINE: {engine}"),
        });
    }

    let host = env_optional("SDKWORK_CLAW_DATABASE_HOST");
    let database = env_optional("SDKWORK_CLAW_DATABASE_NAME");
    let username = env_optional("SDKWORK_CLAW_DATABASE_USERNAME");
    let password = env_optional("SDKWORK_CLAW_DATABASE_PASSWORD");
    let mut missing = Vec::new();
    if host.is_none() {
        missing.push("SDKWORK_CLAW_DATABASE_HOST");
    }
    if database.is_none() {
        missing.push("SDKWORK_CLAW_DATABASE_NAME");
    }
    if username.is_none() {
        missing.push("SDKWORK_CLAW_DATABASE_USERNAME");
    }
    if password.is_none() {
        missing.push("SDKWORK_CLAW_DATABASE_PASSWORD");
    }
    if !missing.is_empty() {
        return Err(ConfigError::MissingRequired(format!(
            "SDKWORK_CLAW_DATABASE_ENGINE=postgresql requires {}",
            missing.join(", ")
        )));
    }

    Ok(Some(build_postgres_database_url(
        host.as_deref().expect("host validated above"),
        env_optional("SDKWORK_CLAW_DATABASE_PORT").as_deref(),
        database.as_deref().expect("database validated above"),
        username.as_deref().expect("username validated above"),
        password.as_deref().expect("password validated above"),
        env_optional("SDKWORK_CLAW_DATABASE_SSL_MODE").as_deref(),
    )))
}

const DEFAULT_POSTGRES_SCHEMA: &str = "public";

/// Resolve the PostgreSQL schema used for application tables in the unified claw profile.
pub fn resolve_unified_postgres_schema(service_prefix: &str) -> String {
    for key in [
        format!("{service_prefix}_DATABASE_SCHEMA"),
        "SDKWORK_CLAW_DATABASE_SCHEMA".to_string(),
        "SDKWORK_DATABASE_SCHEMA".to_string(),
    ] {
        if let Some(value) = env_optional(&key) {
            return value;
        }
    }
    DEFAULT_POSTGRES_SCHEMA.to_string()
}

/// Resolve the canonical sdkwork-claw-router PostgreSQL URL from process env.
pub fn resolve_claw_router_database_url_from_env() -> Result<Option<String>, ConfigError> {
    if let Some(url) = env_optional("SDKWORK_CLAW_DATABASE_URL") {
        return Ok(Some(url));
    }
    resolve_postgres_database_url_from_split_fields()
}

/// Resolve the unified SDKWork database URL for any service, aligned with sdkwork-claw-router.
pub fn resolve_unified_database_url(service_prefix: &str) -> Result<String, ConfigError> {
    let direct_keys = [
        format!("{service_prefix}_DATABASE_URL"),
        "SDKWORK_DATABASE_URL".to_string(),
        "DATABASE_URL".to_string(),
        "SDKWORK_CLAW_DATABASE_URL".to_string(),
    ];
    for key in direct_keys {
        if let Some(url) = env_optional(&key) {
            return Ok(url);
        }
    }

    if let Some(url) = resolve_postgres_database_url_from_split_fields()? {
        return Ok(url);
    }

    Ok(default_claw_router_dev_postgres_database_url())
}

pub fn resolve_unified_max_connections(service_prefix: &str) -> u32 {
    for key in [
        format!("{service_prefix}_DATABASE_MAX_CONNECTIONS"),
        "SDKWORK_CLAW_DATABASE_MAX_CONNECTIONS".to_string(),
    ] {
        if let Some(value) = env_optional(&key) {
            if let Ok(parsed) = value.parse::<u32>() {
                return parsed;
            }
        }
    }
    default_claw_router_dev_postgres_max_connections()
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
    fn resolves_unified_postgres_schema_from_claw_profile() {
        let _lock = lock_env_tests();
        let _guard = EnvGuard::set(&[
            ("SDKWORK_IAM_DATABASE_SCHEMA", None),
            ("SDKWORK_CLAW_DATABASE_SCHEMA", Some("sdkwork_ai_dev")),
            ("SDKWORK_DATABASE_SCHEMA", None),
        ]);

        assert_eq!(
            resolve_unified_postgres_schema("SDKWORK_IAM"),
            "sdkwork_ai_dev"
        );
    }

    #[test]
    fn resolves_unified_postgres_schema_defaults_to_public() {
        let _lock = lock_env_tests();
        let _guard = EnvGuard::set(&[
            ("SDKWORK_IAM_DATABASE_SCHEMA", None),
            ("SDKWORK_CLAW_DATABASE_SCHEMA", None),
            ("SDKWORK_DATABASE_SCHEMA", None),
        ]);

        assert_eq!(resolve_unified_postgres_schema("SDKWORK_IAM"), "public");
    }

    #[test]
    fn default_dev_postgres_url_matches_claw_router_profile() {
        assert_eq!(
            default_claw_router_dev_postgres_database_url(),
            "postgresql://sdkwork_ai_dev:sdkworkdev123@[::1]:5432/sdkwork_ai_dev?sslmode=disable"
        );
    }

    #[test]
    fn resolves_split_claw_fields_into_database_url() {
        let _lock = lock_env_tests();
        let _guard = EnvGuard::set(&[
            ("SDKWORK_SPLITTEST_DATABASE_URL", None),
            ("SDKWORK_DATABASE_URL", None),
            ("DATABASE_URL", None),
            ("SDKWORK_CLAW_DATABASE_URL", None),
            ("SDKWORK_CLAW_DATABASE_ENGINE", Some("postgresql")),
            ("SDKWORK_CLAW_DATABASE_HOST", Some("127.0.0.1")),
            ("SDKWORK_CLAW_DATABASE_PORT", Some("15432")),
            ("SDKWORK_CLAW_DATABASE_NAME", Some("sdkwork_ai_dev")),
            ("SDKWORK_CLAW_DATABASE_USERNAME", Some("sdkwork_ai_dev")),
            ("SDKWORK_CLAW_DATABASE_PASSWORD", Some("sdkworkdev123")),
            ("SDKWORK_CLAW_DATABASE_SSL_MODE", Some("disable")),
        ]);

        let url = resolve_unified_database_url("SDKWORK_SPLITTEST").expect("url should resolve");
        assert_eq!(
            url,
            "postgresql://sdkwork_ai_dev:sdkworkdev123@127.0.0.1:15432/sdkwork_ai_dev?sslmode=disable"
        );
    }

    #[test]
    fn test_service_specific_url_takes_precedence_over_claw_defaults() {
        let _lock = lock_env_tests();
        let _guard = EnvGuard::set(&[
            (
                "SDKWORK_PRECEDENCE_DATABASE_URL",
                Some("postgresql://iam:secret@127.0.0.1:5432/iam_db"),
            ),
            (
                "SDKWORK_CLAW_DATABASE_URL",
                Some("postgresql://ignored/ignored"),
            ),
            ("SDKWORK_DATABASE_URL", None),
            ("DATABASE_URL", None),
            ("SDKWORK_CLAW_DATABASE_HOST", None),
            ("SDKWORK_CLAW_DATABASE_PORT", None),
            ("SDKWORK_CLAW_DATABASE_NAME", None),
            ("SDKWORK_CLAW_DATABASE_USERNAME", None),
            ("SDKWORK_CLAW_DATABASE_PASSWORD", None),
        ]);

        let url = resolve_unified_database_url("SDKWORK_PRECEDENCE").expect("url should resolve");
        assert_eq!(url, "postgresql://iam:secret@127.0.0.1:5432/iam_db");
    }
}
