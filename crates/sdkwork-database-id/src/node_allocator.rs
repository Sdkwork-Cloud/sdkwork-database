//! Database-backed Snowflake node_id allocation with idempotent restart.
//!
//! This module provides automatic, collision-free allocation of Snowflake
//! node IDs (0..1023) from a shared database table. Each process registers
//! its identity and receives a stable node ID that is reclaimed on restart,
//! eliminating the need for manual `SDKWORK_*_ID_NODE_ID` configuration.
//!
//! # How it works
//!
//! 1. On startup, the allocator computes a **process identity** from
//!    `service_name + hostname + optional instance_id`.
//! 2. It checks the `sdkwork_node_registry` table for an existing **active
//!    lease** matching that identity. If found, the same `node_id` is
//!    reclaimed (idempotent restart).
//! 3. If no active lease exists, the smallest available `node_id` is
//!    allocated and inserted.
//! 4. A background heartbeat task periodically renews the lease. If the
//!    process crashes, the lease expires after the TTL and the `node_id`
//!    becomes available for reuse.
//!
//! # Idempotency guarantees
//!
//! - Same `service_name` + same host → same `node_id` on restart.
//! - In Kubernetes, each pod has a unique hostname, so each pod gets a
//!   distinct, stable `node_id`.
//! - For multiple instances on the same host, set
//!   `SDKWORK_NODE_INSTANCE_ID` to disambiguate.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sdkwork_database_sqlx::DatabasePool;
use sdkwork_id_core::{max_snowflake_node_id, SnowflakeIdError, SnowflakeIdGenerator};
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// Default lease time-to-live: 60 seconds.
const DEFAULT_LEASE_TTL: Duration = Duration::from_secs(60);

/// Default heartbeat interval: 20 seconds (TTL / 3).
const DEFAULT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

/// Maximum retry attempts when racing with concurrent allocators.
const MAX_ALLOCATION_RETRIES: usize = 8;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during node_id allocation.
#[derive(Debug)]
pub enum NodeAllocatorError {
    /// A database query failed.
    Database(String),
    /// All 1024 node IDs are in use.
    AllNodeIdsExhausted,
    /// The Snowflake generator rejected the allocated node_id.
    Snowflake(SnowflakeIdError),
    /// The database pool was not available.
    PoolUnavailable,
}

impl std::fmt::Display for NodeAllocatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(msg) => write!(f, "node allocator database error: {msg}"),
            Self::AllNodeIdsExhausted => write!(f, "all 1024 snowflake node IDs are exhausted"),
            Self::Snowflake(err) => write!(f, "snowflake init failed: {err:?}"),
            Self::PoolUnavailable => write!(f, "database pool is unavailable"),
        }
    }
}

impl std::error::Error for NodeAllocatorError {}

impl From<SnowflakeIdError> for NodeAllocatorError {
    fn from(err: SnowflakeIdError) -> Self {
        Self::Snowflake(err)
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for a node allocation request.
#[derive(Debug, Clone)]
pub struct NodeAllocatorConfig {
    /// Logical service name (e.g. `"social-service"`, `"memory-service"`).
    pub service_name: String,
    /// Stable identity used for lease reclamation on restart.
    pub instance_identity: String,
    /// Hostname of the machine / pod.
    pub hostname: String,
    /// How long a lease remains valid without heartbeat.
    pub lease_ttl: Duration,
    /// How often the heartbeat task renews the lease.
    pub heartbeat_interval: Duration,
}

impl NodeAllocatorConfig {
    /// Build a config from a service name, automatically resolving hostname
    /// and instance identity from environment.
    pub fn from_service_name(service_name: &str) -> Self {
        let hostname = resolve_hostname();
        let instance_identity = resolve_instance_identity(service_name, &hostname);
        Self {
            service_name: service_name.to_string(),
            instance_identity,
            hostname,
            lease_ttl: DEFAULT_LEASE_TTL,
            heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
        }
    }

    /// Override the lease TTL.
    #[must_use]
    pub fn with_lease_ttl(mut self, ttl: Duration) -> Self {
        self.lease_ttl = ttl;
        self
    }

    /// Override the heartbeat interval.
    #[must_use]
    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }
}

// ---------------------------------------------------------------------------
// Node lease handle
// ---------------------------------------------------------------------------

/// A handle to an allocated node_id lease.
///
/// While this value is alive, a background heartbeat task keeps the lease
/// renewed. When dropped, the heartbeat task is aborted and the lease will
/// expire after its TTL, allowing another process to claim the node_id.
pub struct NodeLease {
    node_id: u16,
    heartbeat_handle: Option<JoinHandle<()>>,
}

impl NodeLease {
    /// The allocated node_id (0..1023).
    pub fn node_id(&self) -> u16 {
        self.node_id
    }
}

impl Drop for NodeLease {
    fn drop(&mut self) {
        if let Some(handle) = self.heartbeat_handle.take() {
            handle.abort();
        }
    }
}

impl std::fmt::Debug for NodeLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeLease")
            .field("node_id", &self.node_id)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Allocator
// ---------------------------------------------------------------------------

/// Database-backed Snowflake node_id allocator.
///
/// Provides idempotent, collision-free allocation of Snowflake node IDs
/// from a shared `sdkwork_node_registry` table.
pub struct SnowflakeNodeAllocator;

impl SnowflakeNodeAllocator {
    /// Allocate or reclaim a node_id from the database.
    ///
    /// This method is **idempotent**: if the same `instance_identity` has
    /// an active (non-expired) lease, the same `node_id` is reclaimed.
    /// Otherwise, the smallest available `node_id` is allocated.
    ///
    /// A background heartbeat task is started to keep the lease alive.
    /// The task is aborted when the returned [`NodeLease`] is dropped.
    pub async fn allocate(
        pool: &DatabasePool,
        config: &NodeAllocatorConfig,
    ) -> Result<NodeLease, NodeAllocatorError> {
        ensure_registry_table(pool).await?;

        let pid = std::process::id() as i64;
        let started_at_ms = current_epoch_millis();

        for attempt in 0..MAX_ALLOCATION_RETRIES {
            match try_allocate_or_reclaim(pool, config, pid, started_at_ms).await {
                Ok(node_id) => {
                    let heartbeat_handle = start_heartbeat(pool.clone(), node_id, config);
                    info!(
                        node_id,
                        service = %config.service_name,
                        instance = %config.instance_identity,
                        "snowflake node_id allocated"
                    );
                    return Ok(NodeLease {
                        node_id,
                        heartbeat_handle: Some(heartbeat_handle),
                    });
                }
                Err(NodeAllocatorError::Database(_)) if attempt + 1 < MAX_ALLOCATION_RETRIES => {
                    let backoff = Duration::from_millis(50u64 << attempt.min(5));
                    warn!(attempt, ?backoff, "node allocation retry");
                    tokio::time::sleep(backoff).await;
                }
                Err(err) => return Err(err),
            }
        }
        Err(NodeAllocatorError::AllNodeIdsExhausted)
    }

    /// Convenience: allocate a node_id and create a [`SnowflakeIdGenerator`].
    ///
    /// Returns both the generator and the [`NodeLease`] (keep the lease
    /// alive for as long as you need the generator).
    pub async fn allocate_generator(
        pool: &DatabasePool,
        config: &NodeAllocatorConfig,
    ) -> Result<(SnowflakeIdGenerator, NodeLease), NodeAllocatorError> {
        let lease = Self::allocate(pool, config).await?;
        let generator = SnowflakeIdGenerator::new(lease.node_id())?;
        Ok((generator, lease))
    }

    /// High-level: create a pool from environment, allocate, and return
    /// a generator + lease.
    ///
    /// `service_name` is the logical service identifier for the node
    /// registry. `db_service_name` is the SDKWork database service name
    /// used to resolve `SDKWORK_{NAME}_DATABASE_*` env vars (e.g.
    /// `"MEMORY"` for Memory services, `"IM"` for IM services).
    pub async fn allocate_generator_from_env(
        service_name: &str,
        db_service_name: &str,
    ) -> Result<(SnowflakeIdGenerator, NodeLease), NodeAllocatorError> {
        Self::allocate_generator_from_env_with_config(
            service_name,
            db_service_name,
            NodeAllocatorConfig::from_service_name(service_name),
        )
        .await
    }

    /// Like [`allocate_generator_from_env`] but with a custom
    /// [`NodeAllocatorConfig`].
    pub async fn allocate_generator_from_env_with_config(
        service_name: &str,
        db_service_name: &str,
        config: NodeAllocatorConfig,
    ) -> Result<(SnowflakeIdGenerator, NodeLease), NodeAllocatorError> {
        let db_config = sdkwork_database_config::DatabaseConfig::from_env(db_service_name)
            .map_err(|e| NodeAllocatorError::Database(format!("database config: {e}")))?;
        let pool = sdkwork_database_sqlx::create_pool_from_config(db_config)
            .await
            .map_err(|e| NodeAllocatorError::Database(format!("pool creation: {e}")))?;
        let _ = service_name; // used in config, kept for API clarity
        Self::allocate_generator(&pool, &config).await
    }
}

// ---------------------------------------------------------------------------
// Internal: table creation
// ---------------------------------------------------------------------------

/// DDL that works for both PostgreSQL and SQLite.
const CREATE_TABLE_SQL: &str = concat!(
    "CREATE TABLE IF NOT EXISTS sdkwork_node_registry (\n",
    "    node_id INTEGER PRIMARY KEY,\n",
    "    service_name TEXT NOT NULL,\n",
    "    instance_identity TEXT NOT NULL,\n",
    "    hostname TEXT NOT NULL,\n",
    "    pid INTEGER NOT NULL,\n",
    "    started_at_ms INTEGER NOT NULL,\n",
    "    last_heartbeat_at_ms INTEGER NOT NULL,\n",
    "    expires_at_ms INTEGER NOT NULL\n",
    ")"
);

async fn ensure_registry_table(pool: &DatabasePool) -> Result<(), NodeAllocatorError> {
    match pool {
        DatabasePool::Postgres(pg, _) => {
            sqlx::query(CREATE_TABLE_SQL)
                .execute(pg)
                .await
                .map_err(|e| NodeAllocatorError::Database(format!("create table: {e}")))?;
        }
        DatabasePool::Sqlite(sqlite, _) => {
            sqlx::query(CREATE_TABLE_SQL)
                .execute(sqlite)
                .await
                .map_err(|e| NodeAllocatorError::Database(format!("create table: {e}")))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal: allocation logic
// ---------------------------------------------------------------------------

/// Try to allocate or reclaim a node_id. Returns the node_id on success.
async fn try_allocate_or_reclaim(
    pool: &DatabasePool,
    config: &NodeAllocatorConfig,
    pid: i64,
    started_at_ms: i64,
) -> Result<u16, NodeAllocatorError> {
    let now_ms = current_epoch_millis();
    let expires_at_ms = now_ms + config.lease_ttl.as_millis() as i64;

    // 1. Try to reclaim an existing active lease for this instance identity.
    if let Some(node_id) = try_reclaim(pool, &config.instance_identity, now_ms).await? {
        return Ok(node_id);
    }

    // 2. Find the smallest available node_id.
    let active_ids = fetch_active_node_ids(pool, now_ms).await?;
    let candidate = find_first_available(&active_ids);

    let Some(node_id) = candidate else {
        return Err(NodeAllocatorError::AllNodeIdsExhausted);
    };

    // 3. Try to INSERT. On conflict (race with another process), the caller
    //    will retry.
    let inserted = try_insert(
        pool,
        node_id,
        &config.service_name,
        &config.instance_identity,
        &config.hostname,
        pid,
        started_at_ms,
        now_ms,
        expires_at_ms,
    )
    .await?;

    if inserted {
        Ok(node_id)
    } else {
        // Conflict – caller retries.
        Err(NodeAllocatorError::Database(
            "node_id insert conflict, retrying".to_string(),
        ))
    }
}

/// Attempt to reclaim an existing active lease for the given identity.
async fn try_reclaim(
    pool: &DatabasePool,
    instance_identity: &str,
    now_ms: i64,
) -> Result<Option<u16>, NodeAllocatorError> {
    let expires_at_ms = now_ms + DEFAULT_LEASE_TTL.as_millis() as i64;
    let started_at_ms = now_ms;
    let pid = std::process::id() as i64;

    match pool {
        DatabasePool::Postgres(pg, _) => {
            // Try to find an existing active lease.
            let existing: Option<(i64,)> = sqlx::query_as(
                "SELECT node_id FROM sdkwork_node_registry \
                 WHERE instance_identity = $1 AND expires_at_ms > $2",
            )
            .bind(instance_identity)
            .bind(now_ms)
            .fetch_optional(pg)
            .await
            .map_err(|e| NodeAllocatorError::Database(format!("reclaim query: {e}")))?;

            if let Some((node_id,)) = existing {
                // Reclaim: update the lease with new pid and timestamps.
                let _ = sqlx::query(
                    "UPDATE sdkwork_node_registry \
                     SET pid = $1, started_at_ms = $2, last_heartbeat_at_ms = $3, expires_at_ms = $4 \
                     WHERE node_id = $5 AND instance_identity = $6",
                )
                .bind(pid)
                .bind(started_at_ms)
                .bind(now_ms)
                .bind(expires_at_ms)
                .bind(node_id)
                .bind(instance_identity)
                .execute(pg)
                .await
                .map_err(|e| NodeAllocatorError::Database(format!("reclaim update: {e}")))?;
                return Ok(Some(node_id as u16));
            }
        }
        DatabasePool::Sqlite(sqlite, _) => {
            let existing: Option<(i64,)> = sqlx::query_as(
                "SELECT node_id FROM sdkwork_node_registry \
                 WHERE instance_identity = ? AND expires_at_ms > ?",
            )
            .bind(instance_identity)
            .bind(now_ms)
            .fetch_optional(sqlite)
            .await
            .map_err(|e| NodeAllocatorError::Database(format!("reclaim query: {e}")))?;

            if let Some((node_id,)) = existing {
                let _ = sqlx::query(
                    "UPDATE sdkwork_node_registry \
                     SET pid = ?, started_at_ms = ?, last_heartbeat_at_ms = ?, expires_at_ms = ? \
                     WHERE node_id = ? AND instance_identity = ?",
                )
                .bind(pid)
                .bind(started_at_ms)
                .bind(now_ms)
                .bind(expires_at_ms)
                .bind(node_id)
                .bind(instance_identity)
                .execute(sqlite)
                .await
                .map_err(|e| NodeAllocatorError::Database(format!("reclaim update: {e}")))?;
                return Ok(Some(node_id as u16));
            }
        }
    }
    Ok(None)
}

/// Fetch all active (non-expired) node_ids.
async fn fetch_active_node_ids(
    pool: &DatabasePool,
    now_ms: i64,
) -> Result<Vec<u16>, NodeAllocatorError> {
    match pool {
        DatabasePool::Postgres(pg, _) => {
            let rows: Vec<(i64,)> = sqlx::query_as(
                "SELECT node_id FROM sdkwork_node_registry \
                 WHERE expires_at_ms > $1 ORDER BY node_id",
            )
            .bind(now_ms)
            .fetch_all(pg)
            .await
            .map_err(|e| NodeAllocatorError::Database(format!("fetch active ids: {e}")))?;
            Ok(rows.into_iter().map(|(id,)| id as u16).collect())
        }
        DatabasePool::Sqlite(sqlite, _) => {
            let rows: Vec<(i64,)> = sqlx::query_as(
                "SELECT node_id FROM sdkwork_node_registry \
                 WHERE expires_at_ms > ? ORDER BY node_id",
            )
            .bind(now_ms)
            .fetch_all(sqlite)
            .await
            .map_err(|e| NodeAllocatorError::Database(format!("fetch active ids: {e}")))?;
            Ok(rows.into_iter().map(|(id,)| id as u16).collect())
        }
    }
}

/// Find the smallest node_id (0..=max) not present in `active_ids`.
/// `active_ids` must be sorted ascending.
fn find_first_available(active_ids: &[u16]) -> Option<u16> {
    let max = max_snowflake_node_id();
    let mut expected = 0u16;
    for &id in active_ids {
        if id > expected {
            return Some(expected);
        }
        if id == expected {
            expected = expected.checked_add(1)?;
        }
    }
    if expected <= max {
        Some(expected)
    } else {
        None
    }
}

/// Try to INSERT a new lease. Returns `true` if inserted, `false` on conflict.
async fn try_insert(
    pool: &DatabasePool,
    node_id: u16,
    service_name: &str,
    instance_identity: &str,
    hostname: &str,
    pid: i64,
    started_at_ms: i64,
    now_ms: i64,
    expires_at_ms: i64,
) -> Result<bool, NodeAllocatorError> {
    let rows_affected = match pool {
        DatabasePool::Postgres(pg, _) => {
            let result = sqlx::query(
                "INSERT INTO sdkwork_node_registry \
                 (node_id, service_name, instance_identity, hostname, pid, \
                  started_at_ms, last_heartbeat_at_ms, expires_at_ms) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
                 ON CONFLICT (node_id) DO UPDATE SET \
                     service_name = $2, \
                     instance_identity = $3, \
                     hostname = $4, \
                     pid = $5, \
                     started_at_ms = $6, \
                     last_heartbeat_at_ms = $7, \
                     expires_at_ms = $8 \
                 WHERE sdkwork_node_registry.expires_at_ms <= $9",
            )
            .bind(node_id as i64)
            .bind(service_name)
            .bind(instance_identity)
            .bind(hostname)
            .bind(pid)
            .bind(started_at_ms)
            .bind(now_ms)
            .bind(expires_at_ms)
            .bind(now_ms)
            .execute(pg)
            .await
            .map_err(|e| NodeAllocatorError::Database(format!("insert: {e}")))?;
            result.rows_affected()
        }
        DatabasePool::Sqlite(sqlite, _) => {
            let result = sqlx::query(
                "INSERT INTO sdkwork_node_registry \
                 (node_id, service_name, instance_identity, hostname, pid, \
                  started_at_ms, last_heartbeat_at_ms, expires_at_ms) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT (node_id) DO UPDATE SET \
                     service_name = excluded.service_name, \
                     instance_identity = excluded.instance_identity, \
                     hostname = excluded.hostname, \
                     pid = excluded.pid, \
                     started_at_ms = excluded.started_at_ms, \
                     last_heartbeat_at_ms = excluded.last_heartbeat_at_ms, \
                     expires_at_ms = excluded.expires_at_ms \
                 WHERE sdkwork_node_registry.expires_at_ms <= ?",
            )
            .bind(node_id as i64)
            .bind(service_name)
            .bind(instance_identity)
            .bind(hostname)
            .bind(pid)
            .bind(started_at_ms)
            .bind(now_ms)
            .bind(expires_at_ms)
            .bind(now_ms)
            .execute(sqlite)
            .await
            .map_err(|e| NodeAllocatorError::Database(format!("insert: {e}")))?;
            result.rows_affected()
        }
    };
    Ok(rows_affected > 0)
}

// ---------------------------------------------------------------------------
// Internal: heartbeat
// ---------------------------------------------------------------------------

/// Start a background task that periodically renews the lease.
fn start_heartbeat(
    pool: DatabasePool,
    node_id: u16,
    config: &NodeAllocatorConfig,
) -> JoinHandle<()> {
    let interval = config.heartbeat_interval;
    let ttl = config.lease_ttl;
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await; // skip immediate first tick
        loop {
            ticker.tick().await;
            let now_ms = current_epoch_millis();
            let expires_at_ms = now_ms + ttl.as_millis() as i64;
            if let Err(e) = renew_lease(&pool, node_id, now_ms, expires_at_ms).await {
                warn!(node_id, error = %e, "heartbeat renewal failed");
            }
        }
    })
}

/// Renew the lease for a node_id.
async fn renew_lease(
    pool: &DatabasePool,
    node_id: u16,
    now_ms: i64,
    expires_at_ms: i64,
) -> Result<(), String> {
    match pool {
        DatabasePool::Postgres(pg, _) => {
            sqlx::query(
                "UPDATE sdkwork_node_registry \
                 SET last_heartbeat_at_ms = $1, expires_at_ms = $2 \
                 WHERE node_id = $3",
            )
            .bind(now_ms)
            .bind(expires_at_ms)
            .bind(node_id as i64)
            .execute(pg)
            .await
            .map_err(|e| e.to_string())?;
            Ok(())
        }
        DatabasePool::Sqlite(sqlite, _) => {
            sqlx::query(
                "UPDATE sdkwork_node_registry \
                 SET last_heartbeat_at_ms = ?, expires_at_ms = ? \
                 WHERE node_id = ?",
            )
            .bind(now_ms)
            .bind(expires_at_ms)
            .bind(node_id as i64)
            .execute(sqlite)
            .await
            .map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Internal: identity resolution
// ---------------------------------------------------------------------------

/// Resolve the hostname for the current process.
fn resolve_hostname() -> String {
    if let Ok(h) = std::env::var("SDKWORK_NODE_HOSTNAME") {
        let h = h.trim();
        if !h.is_empty() {
            return h.to_string();
        }
    }
    if let Ok(h) = std::env::var("HOSTNAME") {
        let h = h.trim();
        if !h.is_empty() {
            return h.to_string();
        }
    }
    if let Ok(h) = std::env::var("COMPUTERNAME") {
        let h = h.trim();
        if !h.is_empty() {
            return h.to_string();
        }
    }
    "unknown".to_string()
}

/// Resolve a stable instance identity for idempotent lease reclamation.
///
/// Priority:
/// 1. `SDKWORK_NODE_INSTANCE_ID` – explicit per-instance identifier
/// 2. `SDKWORK_IM_SERVICE_NAME` + hostname – IM backward compatibility
/// 3. `service_name` + hostname – default
fn resolve_instance_identity(service_name: &str, hostname: &str) -> String {
    if let Ok(id) = std::env::var("SDKWORK_NODE_INSTANCE_ID") {
        let id = id.trim();
        if !id.is_empty() {
            return format!("{service_name}:{hostname}:{id}");
        }
    }
    if let Ok(im_service) = std::env::var("SDKWORK_IM_SERVICE_NAME") {
        let im_service = im_service.trim();
        if !im_service.is_empty() {
            return format!("{im_service}:{hostname}");
        }
    }
    format!("{service_name}:{hostname}")
}

/// Current epoch time in milliseconds.
fn current_epoch_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_database_config::{DatabaseConfig, DatabaseEngine, DeploymentMode};
    use sdkwork_database_sqlx::create_pool_from_config;

    async fn create_sqlite_pool() -> DatabasePool {
        let config = DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            mode: DeploymentMode::Standalone,
            max_connections: 4,
            ..Default::default()
        };
        create_pool_from_config(config).await.unwrap()
    }

    #[test]
    fn find_first_available_returns_zero_on_empty() {
        assert_eq!(find_first_available(&[]), Some(0));
    }

    #[test]
    fn find_first_available_finds_first_gap() {
        assert_eq!(find_first_available(&[0, 1, 3]), Some(2));
    }

    #[test]
    fn find_first_available_returns_next_after_contiguous() {
        assert_eq!(find_first_available(&[0, 1, 2]), Some(3));
    }

    #[test]
    fn find_first_available_returns_none_when_full() {
        let max = max_snowflake_node_id();
        let all: Vec<u16> = (0..=max).collect();
        assert_eq!(find_first_available(&all), None);
    }

    #[tokio::test]
    async fn allocate_creates_table_and_returns_valid_node_id() {
        let pool = create_sqlite_pool().await;
        let config = NodeAllocatorConfig::from_service_name("test-service");
        let lease = SnowflakeNodeAllocator::allocate(&pool, &config)
            .await
            .unwrap();
        assert!(lease.node_id() <= max_snowflake_node_id());
        pool.close().await;
    }

    #[tokio::test]
    async fn allocate_is_idempotent_on_reclaim() {
        let pool = create_sqlite_pool().await;
        let config = NodeAllocatorConfig::from_service_name("idempotent-test")
            .with_lease_ttl(Duration::from_secs(300))
            .with_heartbeat_interval(Duration::from_secs(120));

        // First allocation.
        let lease1 = SnowflakeNodeAllocator::allocate(&pool, &config)
            .await
            .unwrap();
        let node1 = lease1.node_id();
        drop(lease1); // heartbeat stops, but lease is still active (TTL 300s).

        // Second allocation with same identity should reclaim the same node_id.
        let lease2 = SnowflakeNodeAllocator::allocate(&pool, &config)
            .await
            .unwrap();
        assert_eq!(lease2.node_id(), node1, "should reclaim same node_id");
        drop(lease2);
        pool.close().await;
    }

    #[tokio::test]
    async fn allocate_assigns_different_ids_to_different_services() {
        let pool = create_sqlite_pool().await;
        let config_a = NodeAllocatorConfig::from_service_name("service-a")
            .with_heartbeat_interval(Duration::from_secs(120));
        let config_b = NodeAllocatorConfig::from_service_name("service-b")
            .with_heartbeat_interval(Duration::from_secs(120));

        let lease_a = SnowflakeNodeAllocator::allocate(&pool, &config_a)
            .await
            .unwrap();
        let lease_b = SnowflakeNodeAllocator::allocate(&pool, &config_b)
            .await
            .unwrap();

        assert_ne!(lease_a.node_id(), lease_b.node_id());
        drop(lease_a);
        drop(lease_b);
        pool.close().await;
    }

    #[tokio::test]
    async fn allocate_reuses_expired_node_id() {
        let pool = create_sqlite_pool().await;

        // Create the table first (allocate() would do this, but we need it
        // for our manual insert of an expired lease).
        sqlx::query(CREATE_TABLE_SQL)
            .execute(pool.as_sqlite().unwrap())
            .await
            .unwrap();

        // Insert a lease that is already expired.
        sqlx::query(
            "INSERT INTO sdkwork_node_registry \
             (node_id, service_name, instance_identity, hostname, pid, \
              started_at_ms, last_heartbeat_at_ms, expires_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(0i64)
        .bind("old-service")
        .bind("old-service:unknown:0")
        .bind("unknown")
        .bind(9999i64)
        .bind(1000i64)
        .bind(1000i64)
        .bind(1001i64) // expired
        .execute(pool.as_sqlite().unwrap())
        .await
        .unwrap();

        // New allocation should be able to use node_id 0.
        let config = NodeAllocatorConfig::from_service_name("new-service")
            .with_heartbeat_interval(Duration::from_secs(120));
        let lease = SnowflakeNodeAllocator::allocate(&pool, &config)
            .await
            .unwrap();
        assert_eq!(lease.node_id(), 0);
        drop(lease);
        pool.close().await;
    }

    #[tokio::test]
    async fn allocate_generator_returns_working_generator() {
        let pool = create_sqlite_pool().await;
        let config = NodeAllocatorConfig::from_service_name("gen-test")
            .with_heartbeat_interval(Duration::from_secs(120));

        let (generator, lease) = SnowflakeNodeAllocator::allocate_generator(&pool, &config)
            .await
            .unwrap();

        let id1 = generator.generate().unwrap();
        let id2 = generator.generate().unwrap();
        assert!(id1 > 0);
        assert!(id1 < id2);

        drop(lease);
        pool.close().await;
    }
}
