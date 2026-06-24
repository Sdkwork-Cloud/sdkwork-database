//! Database health check utilities.
//!
//! This module provides utilities for checking database health,
//! including connection status, pool metrics, and query performance.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::error::RepositoryError;

/// Database health status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    /// Database is healthy.
    Healthy,
    /// Database is degraded (e.g., high latency).
    Degraded(String),
    /// Database is unhealthy.
    Unhealthy(String),
}

/// Database health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Overall health status.
    pub status: HealthStatus,
    /// Database engine type.
    pub engine: String,
    /// Connection pool size.
    pub pool_size: u32,
    /// Number of idle connections.
    pub idle_connections: usize,
    /// Query latency in milliseconds.
    pub latency_ms: u64,
    /// Additional details.
    pub details: Option<String>,
}

/// Database health checker.
///
/// Provides methods to check database health and collect metrics.
///
/// # Example
///
/// ```rust,ignore
/// use sdkwork_database_repository::health::HealthChecker;
///
/// let checker = HealthChecker::new(pool);
/// let result = checker.check().await?;
///
/// if result.status == HealthStatus::Healthy {
///     println!("Database is healthy");
/// } else {
///     println!("Database issue: {:?}", result.status);
/// }
/// ```
pub struct HealthChecker {
    pool: sdkwork_database_sqlx::DatabasePool,
}

impl HealthChecker {
    /// Create a new health checker.
    pub fn new(pool: sdkwork_database_sqlx::DatabasePool) -> Self {
        Self { pool }
    }

    /// Perform a health check.
    pub async fn check(&self) -> Result<HealthCheckResult, RepositoryError> {
        // Test connection with a simple query
        let latency = match &self.pool {
            sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                let start = Instant::now();
                sqlx::query("SELECT 1")
                    .execute(pool)
                    .await
                    .map_err(|e| RepositoryError::Database(e))?;
                start.elapsed()
            }
            sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                let start = Instant::now();
                sqlx::query("SELECT 1")
                    .execute(pool)
                    .await
                    .map_err(|e| RepositoryError::Database(e))?;
                start.elapsed()
            }
        };

        let latency_ms = latency.as_millis() as u64;

        // Get pool metrics
        let (pool_size, idle_connections) = match &self.pool {
            sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => (pool.size(), pool.num_idle()),
            sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                (pool.size(), pool.num_idle())
            }
        };

        // Determine health status
        let status = if latency_ms > 1000 {
            HealthStatus::Degraded(format!("High latency: {}ms", latency_ms))
        } else if idle_connections == 0 && pool_size > 0 {
            HealthStatus::Degraded("No idle connections".to_string())
        } else {
            HealthStatus::Healthy
        };

        let engine = match &self.pool {
            sdkwork_database_sqlx::DatabasePool::Sqlite(_, _) => "SQLite",
            sdkwork_database_sqlx::DatabasePool::Postgres(_, _) => "PostgreSQL",
        };

        Ok(HealthCheckResult {
            status,
            engine: engine.to_string(),
            pool_size,
            idle_connections,
            latency_ms,
            details: None,
        })
    }

    /// Check if the database is healthy.
    pub async fn is_healthy(&self) -> bool {
        self.check()
            .await
            .map_or(false, |r| r.status == HealthStatus::Healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_ne!(
            HealthStatus::Healthy,
            HealthStatus::Unhealthy("test".into())
        );
    }
}
