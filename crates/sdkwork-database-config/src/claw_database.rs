use crate::error::ConfigError;

const DEFAULT_DEV_POSTGRES_HOST: &str = "127.0.0.1";
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

/// Build a PostgreSQL URL using the same field semantics as sdkwork-clawrouter.
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

/// Default local PostgreSQL development URL aligned with sdkwork-clawrouter.
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

/// Append a PostgreSQL `options` query parameter so every pooled connection uses the unified schema.
pub fn postgres_url_with_search_path(base_url: &str, service_prefix: &str) -> String {
    let schema = resolve_unified_postgres_schema(service_prefix);
    if schema == DEFAULT_POSTGRES_SCHEMA {
        return base_url.to_string();
    }

    if postgres_url_has_search_path(base_url, schema.as_str()) {
        return base_url.to_string();
    }

    let option_value = format!("-c search_path={schema},public");
    append_postgres_url_query_param(base_url, "options", &option_value)
}

fn postgres_url_has_search_path(base_url: &str, schema: &str) -> bool {
    let Some((_, query_and_fragment)) = base_url.split_once('?') else {
        return false;
    };
    let query = query_and_fragment
        .split_once('#')
        .map_or(query_and_fragment, |(query, _)| query);
    let expected = format!("search_path={schema},public");

    query.split('&').any(|parameter| {
        let Some((key, value)) = parameter.split_once('=') else {
            return false;
        };
        key.eq_ignore_ascii_case("options")
            && percent_decode_uri_component(value).is_some_and(|decoded| {
                decoded
                    .split_ascii_whitespace()
                    .any(|option| option == expected)
            })
    })
}

fn append_postgres_url_query_param(base_url: &str, key: &str, value: &str) -> String {
    let encoded_value = percent_encode_uri_component(value);
    let separator = if base_url.contains('?') { '&' } else { '?' };
    format!("{base_url}{separator}{key}={encoded_value}")
}

fn percent_encode_uri_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode_uri_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => decoded.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let high = decode_hex_digit(bytes[index + 1])?;
                let low = decode_hex_digit(bytes[index + 2])?;
                decoded.push((high << 4) | low);
                index += 2;
            }
            b'%' => return None,
            byte => decoded.push(byte),
        }
        index += 1;
    }
    String::from_utf8(decoded).ok()
}

fn decode_hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

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

/// Resolve the canonical sdkwork-clawrouter PostgreSQL URL from process env.
pub fn resolve_claw_router_database_url_from_env() -> Result<Option<String>, ConfigError> {
    if let Some(url) = env_optional("SDKWORK_CLAW_DATABASE_URL") {
        return Ok(Some(url));
    }
    resolve_postgres_database_url_from_split_fields()
}

/// Resolve the unified SDKWork database URL for any service, aligned with sdkwork-clawrouter.
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
    use serial_test::serial;
    use std::env;

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
    #[serial]
    fn resolves_unified_postgres_schema_from_claw_profile() {
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
    #[serial]
    fn postgres_url_with_search_path_appends_options_for_non_public_schema() {
        let _guard = EnvGuard::set(&[
            ("SDKWORK_IAM_DATABASE_SCHEMA", None),
            ("SDKWORK_CLAW_DATABASE_SCHEMA", Some("sdkwork_ai_dev")),
            ("SDKWORK_DATABASE_SCHEMA", None),
        ]);

        let url = postgres_url_with_search_path(
            "postgresql://sdkwork_ai_dev:sdkworkdev123@127.0.0.1:5432/sdkwork_ai_dev?sslmode=disable",
            "SDKWORK_IAM",
        );
        assert!(url.contains("options=-c%20search_path%3Dsdkwork_ai_dev%2Cpublic"));
    }

    #[test]
    #[serial]
    fn postgres_url_with_search_path_is_idempotent_for_same_schema() {
        let _guard = EnvGuard::set(&[
            ("SDKWORK_IAM_DATABASE_SCHEMA", None),
            ("SDKWORK_CLAW_DATABASE_SCHEMA", Some("sdkwork_ai_dev")),
            ("SDKWORK_DATABASE_SCHEMA", None),
        ]);

        let base =
            "postgresql://sdkwork_ai_dev:sdkworkdev123@127.0.0.1:5432/sdkwork_ai_dev?sslmode=disable";
        let once = postgres_url_with_search_path(base, "SDKWORK_IAM");
        let twice = postgres_url_with_search_path(&once, "SDKWORK_IAM");

        assert_eq!(once, twice);
        assert_eq!(twice.matches("options=").count(), 1);
    }

    #[test]
    #[serial]
    fn repeated_iam_and_single_commerce_normalization_share_authority() {
        let _guard = EnvGuard::set(&[
            ("SDKWORK_IAM_DATABASE_SCHEMA", None),
            ("SDKWORK_COMMERCE_DATABASE_SCHEMA", None),
            ("SDKWORK_CLAW_DATABASE_SCHEMA", Some("sdkwork_ai_dev")),
            ("SDKWORK_DATABASE_SCHEMA", None),
        ]);

        let base =
            "postgresql://sdkwork_ai_dev:sdkworkdev123@127.0.0.1:5432/sdkwork_ai_dev?sslmode=disable";
        let iam_bootstrap_url = postgres_url_with_search_path(base, "SDKWORK_IAM");
        let iam_pool_url = postgres_url_with_search_path(&iam_bootstrap_url, "SDKWORK_IAM");
        let commerce_pool_url = postgres_url_with_search_path(base, "SDKWORK_COMMERCE");

        assert_eq!(iam_pool_url, commerce_pool_url);
    }

    #[test]
    #[serial]
    fn postgres_url_with_search_path_recognizes_plus_encoded_options() {
        let _guard = EnvGuard::set(&[
            ("SDKWORK_IAM_DATABASE_SCHEMA", None),
            ("SDKWORK_CLAW_DATABASE_SCHEMA", Some("sdkwork_ai_dev")),
            ("SDKWORK_DATABASE_SCHEMA", None),
        ]);

        let already_normalized =
            "postgresql://db.internal:5432/app?options=-c+search_path%3Dsdkwork_ai_dev%2Cpublic";
        assert_eq!(
            postgres_url_with_search_path(already_normalized, "SDKWORK_IAM"),
            already_normalized
        );
    }

    #[test]
    #[serial]
    fn postgres_url_with_search_path_appends_when_schema_differs() {
        let _guard = EnvGuard::set(&[
            ("SDKWORK_IAM_DATABASE_SCHEMA", None),
            ("SDKWORK_CLAW_DATABASE_SCHEMA", Some("sdkwork_ai_dev")),
            ("SDKWORK_DATABASE_SCHEMA", None),
        ]);

        let existing =
            "postgresql://db.internal:5432/app?options=-c%20search_path%3Dother_schema%2Cpublic";
        let normalized = postgres_url_with_search_path(existing, "SDKWORK_IAM");

        assert_ne!(normalized, existing);
        assert!(normalized.contains("search_path%3Dother_schema%2Cpublic"));
        assert!(normalized.contains("search_path%3Dsdkwork_ai_dev%2Cpublic"));
    }

    #[test]
    #[serial]
    fn postgres_url_with_search_path_leaves_public_schema_urls_unchanged() {
        let _guard = EnvGuard::set(&[
            ("SDKWORK_IAM_DATABASE_SCHEMA", None),
            ("SDKWORK_CLAW_DATABASE_SCHEMA", None),
            ("SDKWORK_DATABASE_SCHEMA", None),
        ]);

        let base = "postgresql://sdkwork_ai_dev:sdkworkdev123@127.0.0.1:5432/sdkwork_ai_dev";
        assert_eq!(
            base,
            postgres_url_with_search_path(base, "SDKWORK_IAM").as_str()
        );
    }

    #[test]
    #[serial]
    fn resolves_unified_postgres_schema_defaults_to_public() {
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
            "postgresql://sdkwork_ai_dev:sdkworkdev123@127.0.0.1:5432/sdkwork_ai_dev?sslmode=disable"
        );
    }

    #[test]
    #[serial]
    fn resolves_split_claw_fields_into_database_url() {
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
    #[serial]
    fn test_service_specific_url_takes_precedence_over_claw_defaults() {
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
