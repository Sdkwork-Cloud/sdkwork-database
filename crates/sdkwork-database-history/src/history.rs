use sdkwork_database_config::DatabaseEngine;
use sdkwork_database_sqlx::DatabasePool;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::error::HistoryError;

/// --- Framework history tables DDL ---
///
/// These tables track migration and seed application state for the
/// database lifecycle framework. They are created automatically by
/// `ensure_history_tables()`.
///
/// NOTE: In integrated (multi-module) deployment mode, the module
/// prefix SHOULD be applied to these table names to avoid cross-module
/// collisions. A future enhancement should replace the hardcoded table
/// names with module-scoped names resolved through the contract prefix registry.
pub const HISTORY_TABLES_SQL_POSTGRES: &str = r#"
CREATE TABLE IF NOT EXISTS ops_schema_migration_history (
    id BIGSERIAL PRIMARY KEY,
    module_id TEXT NOT NULL,
    version TEXT NOT NULL,
    name TEXT NOT NULL,
    engine TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_by TEXT,
    execution_ms BIGINT,
    UNIQUE(module_id, version, engine)
);

CREATE TABLE IF NOT EXISTS ops_seed_history (
    id BIGSERIAL PRIMARY KEY,
    module_id TEXT NOT NULL,
    seed_id TEXT NOT NULL,
    locale TEXT NOT NULL,
    profile TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_by TEXT,
    UNIQUE(module_id, seed_id, locale, profile)
);

CREATE TABLE IF NOT EXISTS ops_database_installation_state (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    module_id TEXT NOT NULL,
    schema_version TEXT,
    contract_version TEXT,
    catalog_version TEXT,
    environment TEXT,
    seed_locale TEXT,
    seed_profile TEXT,
    status TEXT NOT NULL
);
"#;

pub const HISTORY_TABLES_SQL_SQLITE: &str = r#"
CREATE TABLE IF NOT EXISTS ops_schema_migration_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    module_id TEXT NOT NULL,
    version TEXT NOT NULL,
    name TEXT NOT NULL,
    engine TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now')),
    applied_by TEXT,
    execution_ms INTEGER,
    UNIQUE(module_id, version, engine)
);

CREATE TABLE IF NOT EXISTS ops_seed_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    module_id TEXT NOT NULL,
    seed_id TEXT NOT NULL,
    locale TEXT NOT NULL,
    profile TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now')),
    applied_by TEXT,
    UNIQUE(module_id, seed_id, locale, profile)
);

CREATE TABLE IF NOT EXISTS ops_database_installation_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    module_id TEXT NOT NULL,
    schema_version TEXT,
    contract_version TEXT,
    catalog_version TEXT,
    environment TEXT,
    seed_locale TEXT,
    seed_profile TEXT,
    status TEXT NOT NULL
);
"#;

/// Create lifecycle history tables if they do not exist (using default "ops_" prefix).
///
/// This is the standard entry point for standalone deployments. For multi-module
/// integrated deployments, use `ensure_history_tables_with_prefix` instead.
pub async fn ensure_history_tables(pool: &DatabasePool) -> Result<(), HistoryError> {
    ensure_history_tables_with_prefix(pool, None).await
}

/// Create lifecycle history tables with a custom module prefix for isolation.
///
/// # Arguments
/// * `pool` - The database pool to use
/// * `module_prefix` - Optional module prefix for table names (e.g., "myapp_").
///   If None, uses default "ops_" prefix for backward compatibility.
///
/// # Module Prefix Isolation (ARCH-1)
///
/// In integrated (multi-module) deployment mode, each module SHOULD use a unique
/// prefix to avoid cross-module table name collisions. For example:
/// - Module "iam" uses prefix "iam_" → tables: iam_schema_migration_history, etc.
/// - Module "core" uses prefix "core_" → tables: core_schema_migration_history, etc.
///
/// For standalone deployments, the default "ops_" prefix is recommended.
///
/// # Note
///
/// When using a custom prefix, all history query functions must be updated to use
/// the same prefix. This is a future enhancement tracked in ARCH-1.
pub async fn ensure_history_tables_with_prefix(
    pool: &DatabasePool,
    module_prefix: Option<&str>,
) -> Result<(), HistoryError> {
    let prefix = module_prefix.unwrap_or("ops_");
    let sql = match pool.engine() {
        DatabaseEngine::Postgres => format!(
            r#"
CREATE TABLE IF NOT EXISTS {}schema_migration_history (
    id BIGSERIAL PRIMARY KEY,
    module_id TEXT NOT NULL,
    version TEXT NOT NULL,
    name TEXT NOT NULL,
    engine TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_by TEXT,
    execution_ms BIGINT,
    UNIQUE(module_id, version, engine)
);

CREATE TABLE IF NOT EXISTS {}seed_history (
    id BIGSERIAL PRIMARY KEY,
    module_id TEXT NOT NULL,
    seed_id TEXT NOT NULL,
    locale TEXT NOT NULL,
    profile TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_by TEXT,
    UNIQUE(module_id, seed_id, locale, profile)
);

CREATE TABLE IF NOT EXISTS {}database_installation_state (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    module_id TEXT NOT NULL,
    schema_version TEXT,
    contract_version TEXT,
    catalog_version TEXT,
    environment TEXT,
    seed_locale TEXT,
    seed_profile TEXT,
    status TEXT NOT NULL
);
"#,
            prefix, prefix, prefix
        ),
        DatabaseEngine::Sqlite => format!(
            r#"
CREATE TABLE IF NOT EXISTS {}schema_migration_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    module_id TEXT NOT NULL,
    version TEXT NOT NULL,
    name TEXT NOT NULL,
    engine TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now')),
    applied_by TEXT,
    execution_ms INTEGER,
    UNIQUE(module_id, version, engine)
);

CREATE TABLE IF NOT EXISTS {}seed_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    module_id TEXT NOT NULL,
    seed_id TEXT NOT NULL,
    locale TEXT NOT NULL,
    profile TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now')),
    applied_by TEXT,
    UNIQUE(module_id, seed_id, locale, profile)
);

CREATE TABLE IF NOT EXISTS {}database_installation_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    module_id TEXT NOT NULL,
    schema_version TEXT,
    contract_version TEXT,
    catalog_version TEXT,
    environment TEXT,
    seed_locale TEXT,
    seed_profile TEXT,
    status TEXT NOT NULL
);
"#,
            prefix, prefix, prefix
        ),
    };
    execute_sql_script(pool, &sql).await?;
    Ok(())
}

/// Execute a multi-statement SQL script by splitting on `;` boundaries.
///
/// # Safety
///
/// This function accepts trusted SQL content only (e.g., from migration files
/// that have been checked into version control). User-supplied input MUST NOT
/// be passed to this function — use parameterized queries instead.
pub async fn execute_sql_script(pool: &DatabasePool, script: &str) -> Result<(), HistoryError> {
    for statement in split_sql_statements(script) {
        if statement.is_empty() {
            continue;
        }
        execute_sql(pool, &statement).await?;
    }
    Ok(())
}

/// Execute a single SQL statement against the pool.
///
/// # Safety
///
/// This uses `raw_sql` and MUST only be called with trusted SQL content
/// (version-controlled migration/seed files). DO NOT pass user input here.
pub async fn execute_sql(pool: &DatabasePool, sql: &str) -> Result<(), HistoryError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            sqlx::raw_sql(sql)
                .execute(sqlite_pool)
                .await
                .map_err(|e| HistoryError::Sql(format!("sqlite execute failed: {e}")))?;
        }
        DatabasePool::Postgres(pg_pool, _) => {
            sqlx::raw_sql(sql)
                .execute(pg_pool)
                .await
                .map_err(|e| HistoryError::Sql(format!("postgres execute failed: {e}")))?;
        }
    }
    Ok(())
}

/// Compute SHA-256 checksum of a file on disk.
pub fn file_checksum(path: &std::path::Path) -> Result<String, HistoryError> {
    let bytes = std::fs::read(path)
        .map_err(|e| HistoryError::State(format!("failed to read {}: {e}", path.display())))?;
    let digest = Sha256::digest(bytes);
    Ok(hex::encode(digest))
}

/// List all applied migration version strings for a module/engine.
pub async fn list_applied_migration_versions(
    pool: &DatabasePool,
    module_id: &str,
    engine: DatabaseEngine,
) -> Result<Vec<String>, HistoryError> {
    let engine_name = engine_name_str(engine);
    let query = "SELECT version FROM ops_schema_migration_history \
                  WHERE module_id = $1 AND engine = $2 ORDER BY version";
    fetch_version_column(pool, query, module_id, engine_name).await
}

/// Read the stored checksum for a specific migration, if applied.
pub async fn migration_checksum(
    pool: &DatabasePool,
    module_id: &str,
    version: &str,
    engine: DatabaseEngine,
) -> Result<Option<String>, HistoryError> {
    let engine_name = engine_name_str(engine);
    let query = "SELECT checksum FROM ops_schema_migration_history \
                  WHERE module_id = $1 AND version = $2 AND engine = $3";
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let row = sqlx::query(query)
                .bind(module_id)
                .bind(version)
                .bind(engine_name)
                .fetch_optional(sqlite_pool)
                .await
                .map_err(|e| HistoryError::Sql(e.to_string()))?;
            Ok(row.map(|r| r.get::<String, _>("checksum")))
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let row = sqlx::query(query)
                .bind(module_id)
                .bind(version)
                .bind(engine_name)
                .fetch_optional(pg_pool)
                .await
                .map_err(|e| HistoryError::Sql(e.to_string()))?;
            Ok(row.map(|r| r.get::<String, _>("checksum")))
        }
    }
}

/// Parameters for recording a migration in history.
#[derive(Debug, Clone)]
pub struct MigrationRecord {
    pub module_id: String,
    pub version: String,
    pub name: String,
    pub engine: DatabaseEngine,
    pub checksum: String,
    pub execution_ms: i64,
    pub applied_by: String,
}

impl MigrationRecord {
    /// Create a new migration record.
    pub fn new(
        module_id: impl Into<String>,
        version: impl Into<String>,
        name: impl Into<String>,
        engine: DatabaseEngine,
        checksum: impl Into<String>,
        execution_ms: i64,
        applied_by: impl Into<String>,
    ) -> Self {
        Self {
            module_id: module_id.into(),
            version: version.into(),
            name: name.into(),
            engine,
            checksum: checksum.into(),
            execution_ms,
            applied_by: applied_by.into(),
        }
    }
}

/// Record a migration as applied in the history table.
///
/// # Arguments
/// * `pool` - Database pool
/// * `record` - Migration record parameters
#[allow(clippy::too_many_arguments)]
pub async fn record_migration(
    pool: &DatabasePool,
    module_id: &str,
    version: &str,
    name: &str,
    engine: DatabaseEngine,
    checksum: &str,
    execution_ms: i64,
    applied_by: &str,
) -> Result<(), HistoryError> {
    let engine_name = engine_name_str(engine);

    // Use upsert semantics to handle duplicate inserts gracefully
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            // SQLite: INSERT OR IGNORE to skip duplicates
            sqlx::query(
                "INSERT OR IGNORE INTO ops_schema_migration_history \
                 (module_id, version, name, engine, checksum, applied_by, execution_ms) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(module_id)
            .bind(version)
            .bind(name)
            .bind(engine_name)
            .bind(checksum)
            .bind(applied_by)
            .bind(execution_ms)
            .execute(sqlite_pool)
            .await
            .map_err(|e| HistoryError::Migration(format!("record_migration: {e}")))?;
        }
        DatabasePool::Postgres(pg_pool, _) => {
            // PostgreSQL: ON CONFLICT DO NOTHING
            sqlx::query(
                "INSERT INTO ops_schema_migration_history \
                 (module_id, version, name, engine, checksum, applied_by, execution_ms) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7) \
                 ON CONFLICT (module_id, version, engine) DO NOTHING",
            )
            .bind(module_id)
            .bind(version)
            .bind(name)
            .bind(engine_name)
            .bind(checksum)
            .bind(applied_by)
            .bind(execution_ms)
            .execute(pg_pool)
            .await
            .map_err(|e| HistoryError::Migration(format!("record_migration: {e}")))?;
        }
    }
    Ok(())
}

/// Check whether a seed script has already been applied for the given
/// module, locale, and profile.
pub async fn is_seed_applied(
    pool: &DatabasePool,
    module_id: &str,
    seed_id: &str,
    locale: &str,
    profile: &str,
) -> Result<bool, HistoryError> {
    let query = "SELECT 1 FROM ops_seed_history \
                  WHERE module_id = $1 AND seed_id = $2 AND locale = $3 AND profile = $4 \
                  LIMIT 1";
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let row = sqlx::query(query)
                .bind(module_id)
                .bind(seed_id)
                .bind(locale)
                .bind(profile)
                .fetch_optional(sqlite_pool)
                .await
                .map_err(|e| HistoryError::Seed(e.to_string()))?;
            Ok(row.is_some())
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let row = sqlx::query(query)
                .bind(module_id)
                .bind(seed_id)
                .bind(locale)
                .bind(profile)
                .fetch_optional(pg_pool)
                .await
                .map_err(|e| HistoryError::Seed(e.to_string()))?;
            Ok(row.is_some())
        }
    }
}

/// Record a seed script execution in the history table.
pub async fn record_seed(
    pool: &DatabasePool,
    module_id: &str,
    seed_id: &str,
    locale: &str,
    profile: &str,
    checksum: &str,
    applied_by: &str,
) -> Result<(), HistoryError> {
    let query = "INSERT INTO ops_seed_history \
                  (module_id, seed_id, locale, profile, checksum, applied_by) \
                  VALUES ($1, $2, $3, $4, $5, $6) \
                  ON CONFLICT(module_id, seed_id, locale, profile) DO UPDATE SET \
                      checksum = excluded.checksum, \
                      applied_by = excluded.applied_by";
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            sqlx::query(query)
                .bind(module_id)
                .bind(seed_id)
                .bind(locale)
                .bind(profile)
                .bind(checksum)
                .bind(applied_by)
                .execute(sqlite_pool)
                .await
                .map_err(|e| HistoryError::Seed(e.to_string()))?;
        }
        DatabasePool::Postgres(pg_pool, _) => {
            sqlx::query(query)
                .bind(module_id)
                .bind(seed_id)
                .bind(locale)
                .bind(profile)
                .bind(checksum)
                .bind(applied_by)
                .execute(pg_pool)
                .await
                .map_err(|e| HistoryError::Seed(e.to_string()))?;
        }
    }
    Ok(())
}

/// Upsert the overall installation state for the module.
pub async fn upsert_installation_state(
    pool: &DatabasePool,
    module_id: &str,
    contract_version: &str,
    seed_locale: &str,
    seed_profile: &str,
    status: &str,
) -> Result<(), HistoryError> {
    let query = "INSERT INTO ops_database_installation_state \
                  (id, module_id, contract_version, seed_locale, seed_profile, status) \
                  VALUES (1, $1, $2, $3, $4, $5) \
                  ON CONFLICT(id) DO UPDATE SET \
                      module_id = excluded.module_id, \
                      contract_version = excluded.contract_version, \
                      seed_locale = excluded.seed_locale, \
                      seed_profile = excluded.seed_profile, \
                      status = excluded.status";
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            sqlx::query(query)
                .bind(module_id)
                .bind(contract_version)
                .bind(seed_locale)
                .bind(seed_profile)
                .bind(status)
                .execute(sqlite_pool)
                .await
                .map_err(|e| HistoryError::State(e.to_string()))?;
        }
        DatabasePool::Postgres(pg_pool, _) => {
            sqlx::query(query)
                .bind(module_id)
                .bind(contract_version)
                .bind(seed_locale)
                .bind(seed_profile)
                .bind(status)
                .execute(pg_pool)
                .await
                .map_err(|e| HistoryError::State(e.to_string()))?;
        }
    }
    Ok(())
}

/// Installation state read model.
#[derive(Debug, Clone)]
pub struct InstallationState {
    pub module_id: String,
    pub contract_version: Option<String>,
    pub seed_locale: Option<String>,
    pub seed_profile: Option<String>,
    pub status: String,
}

/// Fetch the current installation state row.
pub async fn fetch_installation_state(
    pool: &DatabasePool,
) -> Result<Option<InstallationState>, HistoryError> {
    let query = "SELECT module_id, contract_version, seed_locale, seed_profile, status \
                  FROM ops_database_installation_state \
                  WHERE id = 1 LIMIT 1";
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let row = sqlx::query(query)
                .fetch_optional(sqlite_pool)
                .await
                .map_err(|e| HistoryError::State(e.to_string()))?;
            Ok(row.map(|r| InstallationState {
                module_id: r.get("module_id"),
                contract_version: r.get("contract_version"),
                seed_locale: r.get("seed_locale"),
                seed_profile: r.get("seed_profile"),
                status: r.get("status"),
            }))
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let row = sqlx::query(query)
                .fetch_optional(pg_pool)
                .await
                .map_err(|e| HistoryError::State(e.to_string()))?;
            Ok(row.map(|r| InstallationState {
                module_id: r.get("module_id"),
                contract_version: r.get("contract_version"),
                seed_locale: r.get("seed_locale"),
                seed_profile: r.get("seed_profile"),
                status: r.get("status"),
            }))
        }
    }
}

/// Applied seed record read model.
#[derive(Debug, Clone)]
pub struct AppliedSeedRecord {
    pub seed_id: String,
    pub locale: String,
    pub profile: String,
    pub checksum: String,
}

/// List all applied seed records for a module.
pub async fn list_applied_seeds(
    pool: &DatabasePool,
    module_id: &str,
) -> Result<Vec<AppliedSeedRecord>, HistoryError> {
    let query = "SELECT seed_id, locale, profile, checksum \
                  FROM ops_seed_history \
                  WHERE module_id = $1 \
                  ORDER BY seed_id, locale, profile";
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let rows = sqlx::query(query)
                .bind(module_id)
                .fetch_all(sqlite_pool)
                .await
                .map_err(|e| HistoryError::Seed(e.to_string()))?;
            Ok(rows
                .iter()
                .map(|r| AppliedSeedRecord {
                    seed_id: r.get("seed_id"),
                    locale: r.get("locale"),
                    profile: r.get("profile"),
                    checksum: r.get("checksum"),
                })
                .collect())
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let rows = sqlx::query(query)
                .bind(module_id)
                .fetch_all(pg_pool)
                .await
                .map_err(|e| HistoryError::Seed(e.to_string()))?;
            Ok(rows
                .iter()
                .map(|r| AppliedSeedRecord {
                    seed_id: r.get("seed_id"),
                    locale: r.get("locale"),
                    profile: r.get("profile"),
                    checksum: r.get("checksum"),
                })
                .collect())
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn engine_name_str(engine: DatabaseEngine) -> &'static str {
    match engine {
        DatabaseEngine::Postgres => "postgres",
        DatabaseEngine::Sqlite => "sqlite",
    }
}

async fn fetch_version_column(
    pool: &DatabasePool,
    query: &str,
    module_id: &str,
    engine_name: &str,
) -> Result<Vec<String>, HistoryError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let rows = sqlx::query(query)
                .bind(module_id)
                .bind(engine_name)
                .fetch_all(sqlite_pool)
                .await
                .map_err(|e| HistoryError::Sql(e.to_string()))?;
            Ok(rows.iter().map(|r| r.get::<String, _>("version")).collect())
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let rows = sqlx::query(query)
                .bind(module_id)
                .bind(engine_name)
                .fetch_all(pg_pool)
                .await
                .map_err(|e| HistoryError::Sql(e.to_string()))?;
            Ok(rows.iter().map(|r| r.get::<String, _>("version")).collect())
        }
    }
}

// ── SQL Statement Splitter ──────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum SqlSplitState {
    Normal,
    SingleQuoted,
    DoubleQuoted,
    DollarQuoted,
    LineComment,
    BlockComment,
}

/// Split a SQL script into individual statements on `;` boundaries,
/// respecting string literals, dollar-quoting, and comments.
///
/// This handles:
/// - Single-quoted strings with escaped quotes (`''`)
/// - Double-quoted identifiers with escaped quotes (`""`)
/// - PostgreSQL dollar-quoting (`$$ ... $$`, `$tag$ ... $tag$`)
/// - PostgreSQL E'' escape strings
/// - Line comments (`--`)
/// - Block comments (`/* ... */`)
fn split_sql_statements(script: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut state = SqlSplitState::Normal;
    let mut dollar_tag: Option<String> = None;
    let bytes = script.as_bytes();
    let len = bytes.len();
    let mut index = 0;

    while index < len {
        let ch = bytes[index] as char;

        match state {
            SqlSplitState::Normal => {
                // Line comment `--`
                if ch == '-' && peek(bytes, index) == Some('-') {
                    push_char(&mut current, ch);
                    push_char(&mut current, '-');
                    index += 2;
                    state = SqlSplitState::LineComment;
                    continue;
                }
                // Block comment `/*`
                if ch == '/' && peek(bytes, index) == Some('*') {
                    push_char(&mut current, ch);
                    push_char(&mut current, '*');
                    index += 2;
                    state = SqlSplitState::BlockComment;
                    continue;
                }
                // E'' escape string (PostgreSQL)
                if ch == 'E' && peek(bytes, index) == Some('\'') {
                    push_char(&mut current, 'E');
                    push_char(&mut current, '\'');
                    index += 2;
                    state = SqlSplitState::SingleQuoted;
                    continue;
                }
                // Single quote
                if ch == '\'' {
                    push_char(&mut current, ch);
                    index += 1;
                    state = SqlSplitState::SingleQuoted;
                    continue;
                }
                // Double quote
                if ch == '"' {
                    push_char(&mut current, ch);
                    index += 1;
                    state = SqlSplitState::DoubleQuoted;
                    continue;
                }
                // Dollar sign — possible dollar-quote start
                if ch == '$' {
                    let (tag, next) = read_dollar_tag(script, index);
                    if tag.len() >= 2 && tag.ends_with('$') {
                        current.push_str(&tag);
                        dollar_tag = Some(tag);
                        index = next;
                        state = SqlSplitState::DollarQuoted;
                        continue;
                    }
                    push_char(&mut current, ch);
                    index += 1;
                    continue;
                }
                // Statement separator
                if ch == ';' {
                    push_trimmed(&mut statements, &current);
                    current.clear();
                    index += 1;
                    continue;
                }
                push_char(&mut current, ch);
                index += 1;
            }
            SqlSplitState::SingleQuoted => {
                if ch == '\'' {
                    if peek(bytes, index) == Some('\'') {
                        // escaped quote ''
                        push_char(&mut current, '\'');
                        push_char(&mut current, '\'');
                        index += 2;
                    } else {
                        push_char(&mut current, '\'');
                        index += 1;
                        state = SqlSplitState::Normal;
                    }
                } else {
                    push_char(&mut current, ch);
                    index += 1;
                }
            }
            SqlSplitState::DoubleQuoted => {
                if ch == '"' {
                    if peek(bytes, index) == Some('"') {
                        // escaped quote ""
                        push_char(&mut current, '"');
                        push_char(&mut current, '"');
                        index += 2;
                    } else {
                        push_char(&mut current, '"');
                        index += 1;
                        state = SqlSplitState::Normal;
                    }
                } else {
                    push_char(&mut current, ch);
                    index += 1;
                }
            }
            SqlSplitState::DollarQuoted => {
                if ch == '$' {
                    let (closing, next) = read_dollar_tag(script, index);
                    current.push_str(&closing);
                    index = next;
                    if dollar_tag.as_deref() == Some(&closing) {
                        dollar_tag = None;
                        state = SqlSplitState::Normal;
                    }
                    continue;
                }
                push_char(&mut current, ch);
                index += 1;
            }
            SqlSplitState::LineComment => {
                push_char(&mut current, ch);
                index += 1;
                if ch == '\n' {
                    state = SqlSplitState::Normal;
                }
            }
            SqlSplitState::BlockComment => {
                if ch == '*' && peek(bytes, index) == Some('/') {
                    push_char(&mut current, '*');
                    push_char(&mut current, '/');
                    index += 2;
                    state = SqlSplitState::Normal;
                } else {
                    push_char(&mut current, ch);
                    index += 1;
                }
            }
        }
    }

    push_trimmed(&mut statements, &current);
    statements
}

fn peek(bytes: &[u8], index: usize) -> Option<char> {
    bytes.get(index + 1).map(|&b| b as char)
}

fn read_dollar_tag(script: &str, start: usize) -> (String, usize) {
    let bytes = script.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if ch == '$' {
            i += 1;
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' {
            i += 1;
        } else {
            break;
        }
    }
    (script[start..i].to_string(), i)
}

fn push_trimmed(statements: &mut Vec<String>, current: &str) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(format!("{trimmed};"));
    }
}

fn push_char(s: &mut String, ch: char) {
    s.push(ch);
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_simple_statements() {
        let stmts = split_sql_statements("SELECT 1; SELECT 2;");
        assert_eq!(stmts, vec!["SELECT 1;", "SELECT 2;"]);
    }

    #[test]
    fn keeps_semicolons_inside_single_quoted_strings() {
        let stmts = split_sql_statements("SELECT 'a;b'; SELECT 2;");
        assert_eq!(stmts, vec!["SELECT 'a;b';", "SELECT 2;"]);
    }

    #[test]
    fn keeps_semicolons_inside_dollar_quoted_blocks() {
        let script = r#"
DO $$
BEGIN
    IF TRUE THEN
        PERFORM 1;
    END IF;
END $$;
SELECT 1;
"#;
        let stmts = split_sql_statements(script);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("DO $$"));
        assert!(stmts[0].contains("END $$;"));
        assert_eq!(stmts[1], "SELECT 1;");
    }

    #[test]
    fn keeps_semicolons_inside_tagged_dollar_quotes() {
        let script = r#"
DO $body$
BEGIN
    PERFORM 1;
END $body$;
SELECT 1;
"#;
        let stmts = split_sql_statements(script);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("DO $body$"));
        assert_eq!(stmts[1], "SELECT 1;");
    }

    #[test]
    fn handles_postgres_escape_string() {
        let script = r#"SELECT E'hello;world'; SELECT 2;"#;
        let stmts = split_sql_statements(script);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("E'hello;world'"));
        assert_eq!(stmts[1], "SELECT 2;");
    }

    #[test]
    fn handles_escaped_quotes() {
        let script = "SELECT 'it''s'; SELECT 2;";
        let stmts = split_sql_statements(script);
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT 'it''s';");
        assert_eq!(stmts[1], "SELECT 2;");
    }

    #[test]
    fn keeps_semicolons_inside_dollar_quoted_blocks_with_crlf() {
        let script = "DO $$\r\nBEGIN\r\n    IF TRUE THEN\r\n        PERFORM 1;\r\n    END IF;\r\nEND $$;\r\nSELECT 1;\r\n";
        let stmts = split_sql_statements(script);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].starts_with("DO $$"));
        assert!(stmts[0].contains("END $$;"));
        assert_eq!(stmts[1], "SELECT 1;");
    }

    #[test]
    fn test_engine_name_str() {
        assert_eq!(engine_name_str(DatabaseEngine::Sqlite), "sqlite");
        assert_eq!(engine_name_str(DatabaseEngine::Postgres), "postgres");
    }
}
