use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::AnyPool;
use std::str::FromStr;
use tracing::info;

use sdkwork_database_config::DatabaseConfig;

use crate::error::PoolError;

/// Create a sqlx AnyPool from SDKWork database configuration.
pub async fn create_any_pool(config: &DatabaseConfig) -> Result<AnyPool, PoolError> {
    sqlx::any::install_default_drivers();

    let connect_options = AnyConnectOptions::from_str(&config.url)
        .map_err(|error| PoolError::InvalidUrl(format!("{}: {}", config.url, error)))?;

    let pool = AnyPoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout())
        .idle_timeout(config.idle_timeout())
        .max_lifetime(config.max_lifetime())
        .connect_with(connect_options)
        .await
        .map_err(PoolError::PoolCreation)?;

    info!(
        engine = ?config.engine,
        url = %mask_url(&config.url),
        max_connections = config.max_connections,
        "Any database connection pool created"
    );

    Ok(pool)
}

fn mask_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(scheme_end) = url.find("://") {
            let scheme = &url[..scheme_end + 3];
            let host_and_rest = &url[at_pos..];
            return format!("{}***:***{}", scheme, host_and_rest);
        }
    }
    url.to_string()
}
