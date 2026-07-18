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
    if script_has_explicit_transaction(script) {
        return execute_transactional_sql_script(pool, script).await;
    }

    for statement in split_sql_statements(script) {
        if statement.is_empty() {
            continue;
        }
        execute_sql(pool, &statement).await?;
    }
    Ok(())
}

/// Execute a trusted lifecycle script as one transaction when the script does not
/// already own an explicit transaction boundary.
pub async fn execute_sql_script_atomically(
    pool: &DatabasePool,
    script: &str,
) -> Result<(), HistoryError> {
    if script_has_explicit_transaction(script) {
        return execute_transactional_sql_script(pool, script).await;
    }

    if let DatabasePool::Sqlite(sqlite_pool, _) = pool {
        return execute_sqlite_script_atomically(sqlite_pool, script).await;
    }

    let transactional_script = format!("BEGIN;\n{script}\nCOMMIT;");
    execute_transactional_sql_script(pool, &transactional_script).await
}

async fn execute_sqlite_script_atomically(
    pool: &sqlx::SqlitePool,
    script: &str,
) -> Result<(), HistoryError> {
    let mut connection = pool.acquire().await.map_err(|error| {
        HistoryError::Sql(format!("sqlite atomic script connection failed: {error}"))
    })?;
    sqlx::raw_sql("BEGIN IMMEDIATE;")
        .execute(&mut *connection)
        .await
        .map_err(|error| {
            HistoryError::Sql(format!("sqlite atomic script begin failed: {error}"))
        })?;

    for statement in split_sql_statements(script) {
        if statement.is_empty() {
            continue;
        }
        if let Err(error) = execute_sqlite_statement(&mut connection, &statement).await {
            let rollback = sqlx::raw_sql("ROLLBACK;").execute(&mut *connection).await;
            if rollback.is_err() {
                connection.close_on_drop();
            }
            return Err(error);
        }
    }

    sqlx::raw_sql("COMMIT;")
        .execute(&mut *connection)
        .await
        .map_err(|error| {
            HistoryError::Sql(format!("sqlite atomic script commit failed: {error}"))
        })?;
    Ok(())
}

async fn execute_sqlite_statement(
    connection: &mut sqlx::SqliteConnection,
    sql: &str,
) -> Result<(), HistoryError> {
    if let Some(statement) = parse_sqlite_add_column_if_not_exists(sql) {
        for column in statement.columns {
            if sqlite_column_exists_on_connection(
                connection,
                &statement.table_name,
                &column.column_name,
            )
            .await?
            {
                continue;
            }
            let add_column_sql = format!(
                "ALTER TABLE {} ADD COLUMN {} {};",
                quote_sqlite_identifier(&statement.table_name),
                quote_sqlite_identifier(&column.column_name),
                column.column_definition_tail
            );
            sqlx::raw_sql(&add_column_sql)
                .execute(&mut *connection)
                .await
                .map_err(|error| {
                    HistoryError::Sql(format!("sqlite add column if not exists failed: {error}"))
                })?;
        }
        return Ok(());
    }

    sqlx::raw_sql(sql)
        .execute(connection)
        .await
        .map_err(|error| HistoryError::Sql(format!("sqlite atomic statement failed: {error}")))?;
    Ok(())
}

async fn execute_transactional_sql_script(
    pool: &DatabasePool,
    script: &str,
) -> Result<(), HistoryError> {
    let (prelude, transactional_script) = split_transaction_prelude(script);
    for statement in split_sql_statements(prelude) {
        if statement.is_empty() {
            continue;
        }
        execute_sql(pool, &statement).await?;
    }

    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let mut connection = sqlite_pool.acquire().await.map_err(|error| {
                HistoryError::Sql(format!(
                    "sqlite transactional script connection failed: {error}"
                ))
            })?;
            if let Err(error) = sqlx::raw_sql(transactional_script)
                .execute(&mut *connection)
                .await
            {
                let rollback = sqlx::raw_sql("ROLLBACK; PRAGMA foreign_keys = ON;")
                    .execute(&mut *connection)
                    .await;
                if rollback.is_err() {
                    connection.close_on_drop();
                }
                return Err(HistoryError::Sql(format!(
                    "sqlite transactional script execution failed: {error}"
                )));
            }
        }
        DatabasePool::Postgres(postgres_pool, _) => {
            let mut connection = postgres_pool.acquire().await.map_err(|error| {
                HistoryError::Sql(format!(
                    "postgres transactional script connection failed: {error}"
                ))
            })?;
            if let Err(error) = sqlx::raw_sql(transactional_script)
                .execute(&mut *connection)
                .await
            {
                if sqlx::query("ROLLBACK")
                    .execute(&mut *connection)
                    .await
                    .is_err()
                {
                    connection.close_on_drop();
                }
                return Err(HistoryError::Sql(format!(
                    "postgres transactional script execution failed: {error}"
                )));
            }
        }
    }
    Ok(())
}

/// Return a prelude that must use the normal statement executor (notably
/// SQLite's compatibility implementation for `ADD COLUMN IF NOT EXISTS`) and
/// the explicit transaction body that must be sent as one raw batch. SQLite
/// pragmas are kept with the transaction body so they run on the same
/// connection as `BEGIN IMMEDIATE`.
fn split_transaction_prelude(script: &str) -> (&str, &str) {
    let mut offset = 0;
    let mut begin_offset = None;
    let mut pragma_offset = None;

    for segment in script.split_inclusive('\n') {
        let trimmed = segment.trim();
        let upper = trimmed.to_ascii_uppercase();
        if begin_offset.is_none()
            && (upper == "BEGIN" || upper == "BEGIN;" || upper.starts_with("BEGIN IMMEDIATE"))
        {
            begin_offset = Some(offset);
        }
        if begin_offset.is_none() && upper.starts_with("PRAGMA FOREIGN_KEYS") {
            pragma_offset = Some(offset);
        }
        offset += segment.len();
    }

    let transaction_offset = pragma_offset.or(begin_offset).unwrap_or(0);
    script.split_at(transaction_offset)
}

fn script_has_explicit_transaction(script: &str) -> bool {
    let mut has_begin = false;
    let mut has_commit = false;
    for line in script.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        has_begin |= upper == "BEGIN;" || upper == "BEGIN" || upper.starts_with("BEGIN IMMEDIATE");
        has_commit |= upper == "COMMIT;" || upper == "COMMIT";
    }
    has_begin && has_commit
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
            if execute_sqlite_add_column_if_not_exists(sqlite_pool, sql).await? {
                return Ok(());
            }
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

struct SqliteAddColumnIfNotExists {
    table_name: String,
    columns: Vec<SqliteAddColumnClause>,
}

struct SqliteAddColumnClause {
    column_name: String,
    column_definition_tail: String,
}

async fn execute_sqlite_add_column_if_not_exists(
    pool: &sqlx::SqlitePool,
    sql: &str,
) -> Result<bool, HistoryError> {
    let Some(statement) = parse_sqlite_add_column_if_not_exists(sql) else {
        return Ok(false);
    };

    for column in statement.columns {
        if sqlite_column_exists(pool, &statement.table_name, &column.column_name).await? {
            continue;
        }
        let add_column_sql = format!(
            "ALTER TABLE {} ADD COLUMN {} {};",
            quote_sqlite_identifier(&statement.table_name),
            quote_sqlite_identifier(&column.column_name),
            column.column_definition_tail
        );
        sqlx::raw_sql(&add_column_sql)
            .execute(pool)
            .await
            .map_err(|error| {
                HistoryError::Sql(format!("sqlite add column if not exists failed: {error}"))
            })?;
    }

    Ok(true)
}

async fn sqlite_column_exists(
    pool: &sqlx::SqlitePool,
    table_name: &str,
    column_name: &str,
) -> Result<bool, HistoryError> {
    let query = format!(
        "SELECT COUNT(*) FROM pragma_table_info({}) WHERE name = $1",
        quote_sqlite_string_literal(table_name)
    );
    let count = sqlx::query_scalar::<_, i64>(&query)
        .bind(column_name)
        .fetch_one(pool)
        .await
        .map_err(|error| {
            HistoryError::Sql(format!("sqlite column existence check failed: {error}"))
        })?;
    Ok(count > 0)
}

async fn sqlite_column_exists_on_connection(
    connection: &mut sqlx::SqliteConnection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, HistoryError> {
    let query = format!(
        "SELECT COUNT(*) FROM pragma_table_info({}) WHERE name = $1",
        quote_sqlite_string_literal(table_name)
    );
    let count = sqlx::query_scalar::<_, i64>(&query)
        .bind(column_name)
        .fetch_one(connection)
        .await
        .map_err(|error| {
            HistoryError::Sql(format!("sqlite column existence check failed: {error}"))
        })?;
    Ok(count > 0)
}

fn parse_sqlite_add_column_if_not_exists(sql: &str) -> Option<SqliteAddColumnIfNotExists> {
    let trimmed = strip_leading_sql_comments(sql.trim().trim_end_matches(';').trim());
    let rest = consume_keyword(trimmed, "ALTER")?;
    let rest = consume_keyword(rest, "TABLE")?;
    let (table_name, rest) = parse_identifier(rest)?;
    let clauses = split_top_level_commas(rest.trim());
    let mut columns = Vec::new();

    for clause in clauses {
        let clause = clause.trim();
        let rest = consume_keyword(clause, "ADD")?;
        let rest = consume_keyword(rest, "COLUMN")?;
        let rest = consume_keyword(rest, "IF")?;
        let rest = consume_keyword(rest, "NOT")?;
        let rest = consume_keyword(rest, "EXISTS")?;
        let (column_name, definition_tail) = parse_identifier(rest)?;
        let definition_tail = definition_tail.trim();
        if definition_tail.is_empty() {
            return None;
        }
        columns.push(SqliteAddColumnClause {
            column_name,
            column_definition_tail: definition_tail.to_string(),
        });
    }

    if columns.is_empty() {
        return None;
    }

    Some(SqliteAddColumnIfNotExists {
        table_name,
        columns,
    })
}

fn consume_keyword<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    let input = input.trim_start();
    let prefix = input.get(..keyword.len())?;
    if !prefix.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let remainder = &input[keyword.len()..];
    if remainder
        .chars()
        .next()
        .is_some_and(is_identifier_character)
    {
        return None;
    }
    Some(remainder)
}

fn parse_identifier(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    let mut chars = input.char_indices();
    let (_, first) = chars.next()?;

    match first {
        '"' | '`' => parse_quoted_identifier(input, first, first),
        '[' => parse_quoted_identifier(input, '[', ']'),
        _ => {
            let end = input
                .char_indices()
                .find_map(|(index, ch)| (!is_identifier_character(ch)).then_some(index))
                .unwrap_or(input.len());
            if end == 0 {
                return None;
            }
            Some((input[..end].to_string(), &input[end..]))
        }
    }
}

fn parse_quoted_identifier(input: &str, open: char, close: char) -> Option<(String, &str)> {
    let start_len = open.len_utf8();
    let mut value = String::new();
    let mut index = start_len;
    while index < input.len() {
        let ch = input[index..].chars().next()?;
        let ch_len = ch.len_utf8();
        if ch == close {
            let next_index = index + ch_len;
            if input[next_index..].starts_with(close) && close != ']' {
                value.push(close);
                index = next_index + close.len_utf8();
                continue;
            }
            return Some((value, &input[next_index..]));
        }
        value.push(ch);
        index += ch_len;
    }
    None
}

fn is_identifier_character(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '.')
}

fn strip_leading_sql_comments(mut input: &str) -> &str {
    loop {
        input = input.trim_start();
        if let Some(after_comment) = input.strip_prefix("--") {
            let Some(newline_index) = after_comment.find('\n') else {
                return "";
            };
            input = &after_comment[newline_index + 1..];
            continue;
        }
        if let Some(after_comment) = input.strip_prefix("/*") {
            let Some(end_index) = after_comment.find("*/") else {
                return input;
            };
            input = &after_comment[end_index + 2..];
            continue;
        }
        return input;
    }
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut paren_depth = 0_u32;
    let mut state = SqlSplitState::Normal;
    let bytes = input.as_bytes();

    while index < input.len() {
        let ch = bytes[index] as char;
        match state {
            SqlSplitState::Normal => {
                if ch == '\'' {
                    index += 1;
                    state = SqlSplitState::SingleQuoted;
                    continue;
                }
                if ch == '"' {
                    index += 1;
                    state = SqlSplitState::DoubleQuoted;
                    continue;
                }
                match ch {
                    '(' => paren_depth += 1,
                    ')' => paren_depth = paren_depth.saturating_sub(1),
                    ',' if paren_depth == 0 => {
                        parts.push(&input[start..index]);
                        start = index + 1;
                    }
                    _ => {}
                }
                index += 1;
            }
            SqlSplitState::SingleQuoted => {
                if ch == '\'' {
                    if bytes.get(index + 1).copied() == Some(b'\'') {
                        index += 2;
                    } else {
                        index += 1;
                        state = SqlSplitState::Normal;
                    }
                } else {
                    index += 1;
                }
            }
            SqlSplitState::DoubleQuoted => {
                if ch == '"' {
                    if bytes.get(index + 1).copied() == Some(b'"') {
                        index += 2;
                    } else {
                        index += 1;
                        state = SqlSplitState::Normal;
                    }
                } else {
                    index += 1;
                }
            }
            SqlSplitState::DollarQuoted
            | SqlSplitState::LineComment
            | SqlSplitState::BlockComment => {
                index += 1;
            }
        }
    }

    parts.push(&input[start..]);
    parts
}

fn quote_sqlite_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_sqlite_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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

    // Keep the unique-key race safe without silently accepting a different
    // checksum.  A duplicate with the same checksum is idempotent; a
    // duplicate with a changed checksum is an integrity failure that must be
    // surfaced to the lifecycle caller.
    let rows_affected = match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => sqlx::query(
            "INSERT INTO ops_schema_migration_history \
                 (module_id, version, name, engine, checksum, applied_by, execution_ms) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7) \
                 ON CONFLICT(module_id, version, engine) DO NOTHING",
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
        .map_err(|e| HistoryError::Migration(format!("record_migration: {e}")))?
        .rows_affected(),
        DatabasePool::Postgres(pg_pool, _) => sqlx::query(
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
        .map_err(|e| HistoryError::Migration(format!("record_migration: {e}")))?
        .rows_affected(),
    };

    if rows_affected == 0 {
        match migration_checksum(pool, module_id, version, engine).await? {
            Some(existing_checksum) if existing_checksum == checksum => {}
            Some(existing_checksum) => {
                return Err(HistoryError::Migration(format!(
                    "checksum_mismatch for migration {version}: applied={existing_checksum}, current={checksum}"
                )));
            }
            None => {
                return Err(HistoryError::Migration(format!(
                    "migration_history_conflict for migration {version}: unique key conflict without a history row"
                )));
            }
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
///
/// UTF-8 safety: the byte-level scan is safe because all SQL delimiter
/// characters (`;`, `'`, `"`, `-`, `/`, `$`, `E`, `*`, `\n`) are ASCII
/// (single-byte in UTF-8). Non-ASCII bytes are always in the catch-all
/// branch and are pushed via `push_script_char` which preserves the full
/// multi-byte UTF-8 character.
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
                    current.push('-');
                    current.push('-');
                    index += 2;
                    state = SqlSplitState::LineComment;
                    continue;
                }
                // Block comment `/*`
                if ch == '/' && peek(bytes, index) == Some('*') {
                    current.push('/');
                    current.push('*');
                    index += 2;
                    state = SqlSplitState::BlockComment;
                    continue;
                }
                // E'' escape string (PostgreSQL)
                if ch == 'E' && peek(bytes, index) == Some('\'') {
                    current.push('E');
                    current.push('\'');
                    index += 2;
                    state = SqlSplitState::SingleQuoted;
                    continue;
                }
                // Single quote
                if ch == '\'' {
                    current.push(ch);
                    index += 1;
                    state = SqlSplitState::SingleQuoted;
                    continue;
                }
                // Double quote
                if ch == '"' {
                    current.push(ch);
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
                    current.push(ch);
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
                push_script_char(&mut current, script, &mut index);
            }
            SqlSplitState::SingleQuoted => {
                if ch == '\'' {
                    if peek(bytes, index) == Some('\'') {
                        // escaped quote ''
                        current.push('\'');
                        current.push('\'');
                        index += 2;
                    } else {
                        current.push('\'');
                        index += 1;
                        state = SqlSplitState::Normal;
                    }
                } else {
                    push_script_char(&mut current, script, &mut index);
                }
            }
            SqlSplitState::DoubleQuoted => {
                if ch == '"' {
                    if peek(bytes, index) == Some('"') {
                        // escaped quote ""
                        current.push('"');
                        current.push('"');
                        index += 2;
                    } else {
                        current.push('"');
                        index += 1;
                        state = SqlSplitState::Normal;
                    }
                } else {
                    push_script_char(&mut current, script, &mut index);
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
                push_script_char(&mut current, script, &mut index);
            }
            SqlSplitState::LineComment => {
                push_script_char(&mut current, script, &mut index);
                if ch == '\n' {
                    state = SqlSplitState::Normal;
                }
            }
            SqlSplitState::BlockComment => {
                if ch == '*' && peek(bytes, index) == Some('/') {
                    current.push('*');
                    current.push('/');
                    index += 2;
                    state = SqlSplitState::Normal;
                } else {
                    push_script_char(&mut current, script, &mut index);
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

/// Push the character at byte `*index` in `script` into `current`,
/// correctly handling multi-byte UTF-8 sequences, and advance `*index`
/// past the character.
///
/// This is used in the catch-all branches of `split_sql_statements`
/// where non-ASCII bytes (continuation bytes of multi-byte UTF-8
/// characters) must be preserved as-is rather than being interpreted
/// as individual Latin-1 characters.
fn push_script_char(current: &mut String, script: &str, index: &mut usize) {
    let ch = script[*index..].chars().next().expect("valid UTF-8");
    current.push(ch);
    *index += ch.len_utf8();
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
    fn preserves_utf8_multibyte_characters_in_string_literals() {
        // Chinese characters inside single-quoted string literals must be
        // preserved exactly — no double-encoding (mojibake).
        let script = "INSERT INTO plans (name) VALUES ('基础会员'); SELECT 1;";
        let stmts = split_sql_statements(script);
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "INSERT INTO plans (name) VALUES ('基础会员');");
        assert_eq!(stmts[1], "SELECT 1;");
    }

    #[test]
    fn preserves_utf8_multibyte_characters_in_identifiers() {
        // Non-ASCII characters outside string literals (e.g. in comments
        // or identifiers) must also be preserved.
        let script = "SELECT 1; -- 中文注释\nSELECT 2;";
        let stmts = split_sql_statements(script);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("SELECT 1"));
        assert!(stmts[1].contains("SELECT 2"));
        // The comment with Chinese characters should be in the second statement
        // (it appears after the first `;` separator).
        assert!(stmts[1].contains("中文注释"));
    }

    #[tokio::test]
    async fn executes_sql_script_with_utf8_string_literals() {
        let config = sdkwork_database_config::DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };
        let pool = sdkwork_database_sqlx::create_pool_from_config(config)
            .await
            .expect("sqlite pool");

        execute_sql_script(
            &pool,
            "CREATE TABLE probe (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
        )
        .await
        .expect("create table");

        execute_sql_script(
            &pool,
            "INSERT INTO probe (id, name) VALUES (1, '基础会员');",
        )
        .await
        .expect("insert with Chinese text");

        let sqlite = pool.as_sqlite().expect("sqlite pool");
        let name = sqlx::query_scalar::<_, String>("SELECT name FROM probe WHERE id = 1")
            .fetch_one(sqlite)
            .await
            .expect("select name");
        assert_eq!(name, "基础会员");
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
    fn detects_only_scripts_with_an_explicit_begin_and_commit() {
        assert!(script_has_explicit_transaction(
            "PRAGMA foreign_keys = OFF;\nBEGIN IMMEDIATE;\nCREATE TABLE probe (id INTEGER);\nCOMMIT;"
        ));
        assert!(script_has_explicit_transaction(
            "BEGIN;\nDO $$\nBEGIN\n  PERFORM 1;\nEND $$;\nCOMMIT;"
        ));
        assert!(!script_has_explicit_transaction(
            "DO $$\nBEGIN\n  PERFORM 1;\nEND $$;"
        ));
    }

    #[tokio::test]
    async fn executes_explicit_transaction_script_as_one_raw_batch() {
        let config = sdkwork_database_config::DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };
        let pool = sdkwork_database_sqlx::create_pool_from_config(config)
            .await
            .expect("sqlite pool");

        execute_sql_script(
            &pool,
            "BEGIN IMMEDIATE; CREATE TABLE probe (id INTEGER PRIMARY KEY); INSERT INTO probe VALUES (1); COMMIT;",
        )
        .await
        .expect("transactional script should execute");

        let sqlite = pool.as_sqlite().expect("sqlite pool");
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM probe")
            .fetch_one(sqlite)
            .await
            .expect("probe count");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn executes_sqlite_compatibility_prelude_before_transaction_batch() {
        let config = sdkwork_database_config::DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };
        let pool = sdkwork_database_sqlx::create_pool_from_config(config)
            .await
            .expect("sqlite pool");
        execute_sql_script(&pool, "CREATE TABLE probe (id INTEGER PRIMARY KEY);")
            .await
            .expect("probe table");

        execute_sql_script(
            &pool,
            r#"
ALTER TABLE probe ADD COLUMN IF NOT EXISTS snapshot TEXT;
ALTER TABLE probe ADD COLUMN IF NOT EXISTS snapshot TEXT;
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;
INSERT INTO probe (id, snapshot) VALUES (1, 'original');
COMMIT;
PRAGMA foreign_keys = ON;
"#,
        )
        .await
        .expect("prelude plus transactional script should execute");

        let sqlite = pool.as_sqlite().expect("sqlite pool");
        let snapshot = sqlx::query_scalar::<_, String>("SELECT snapshot FROM probe WHERE id = 1")
            .fetch_one(sqlite)
            .await
            .expect("snapshot");
        assert_eq!(snapshot, "original");
    }

    #[tokio::test]
    async fn failed_sqlite_transaction_is_rolled_back_before_pool_reuse() {
        let config = sdkwork_database_config::DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };
        let pool = sdkwork_database_sqlx::create_pool_from_config(config)
            .await
            .expect("sqlite pool");

        let error = execute_sql_script(
            &pool,
            r#"
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;
CREATE TABLE failed_probe (id INTEGER PRIMARY KEY);
INSERT INTO failed_probe (id) VALUES (1);
INSERT INTO failed_probe (id) VALUES (1);
COMMIT;
PRAGMA foreign_keys = ON;
"#,
        )
        .await
        .expect_err("duplicate key must abort the transaction");
        assert!(error
            .to_string()
            .contains("transactional script execution failed"));

        let sqlite = pool.as_sqlite().expect("sqlite pool");
        let table_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'failed_probe'",
        )
        .fetch_one(sqlite)
        .await
        .expect("table probe");
        assert_eq!(table_count, 0, "failed transaction must roll back DDL");

        let foreign_keys = sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(sqlite)
            .await
            .expect("foreign key pragma");
        assert_eq!(foreign_keys, 1, "pooled connection must be restored");
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

    #[tokio::test]
    async fn creates_default_history_tables_on_sqlite() {
        let config = sdkwork_database_config::DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };
        let pool = sdkwork_database_sqlx::create_pool_from_config(config)
            .await
            .expect("sqlite pool");

        ensure_history_tables(&pool)
            .await
            .expect("history tables should be created");

        let sqlite = pool.as_sqlite().expect("sqlite pool");
        for table_name in [
            "ops_schema_migration_history",
            "ops_seed_history",
            "ops_database_installation_state",
        ] {
            let present = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = $1",
            )
            .bind(table_name)
            .fetch_one(sqlite)
            .await
            .expect("table probe");
            assert_eq!(present, 1, "{table_name} should exist");
        }
    }

    #[tokio::test]
    async fn record_migration_rejects_conflicting_checksum_instead_of_ignoring_it() {
        let config = sdkwork_database_config::DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };
        let pool = sdkwork_database_sqlx::create_pool_from_config(config)
            .await
            .expect("sqlite pool");
        ensure_history_tables(&pool)
            .await
            .expect("history tables should be created");

        record_migration(
            &pool,
            "demo",
            "0001",
            "create_demo",
            DatabaseEngine::Sqlite,
            "checksum-a",
            1,
            "test",
        )
        .await
        .expect("first migration record");
        record_migration(
            &pool,
            "demo",
            "0001",
            "create_demo",
            DatabaseEngine::Sqlite,
            "checksum-a",
            1,
            "test",
        )
        .await
        .expect("same checksum is idempotent");

        let error = record_migration(
            &pool,
            "demo",
            "0001",
            "create_demo",
            DatabaseEngine::Sqlite,
            "checksum-b",
            1,
            "test",
        )
        .await
        .expect_err("changed checksum must fail");
        assert!(error.to_string().contains("checksum_mismatch"));
    }

    #[tokio::test]
    async fn sqlite_execute_sql_script_supports_add_column_if_not_exists() {
        let config = sdkwork_database_config::DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };
        let pool = sdkwork_database_sqlx::create_pool_from_config(config)
            .await
            .expect("sqlite pool");

        execute_sql_script(
            &pool,
            r#"
            CREATE TABLE probe (id TEXT PRIMARY KEY);
            ALTER TABLE probe ADD COLUMN IF NOT EXISTS display_name TEXT NOT NULL DEFAULT '';
            ALTER TABLE probe ADD COLUMN IF NOT EXISTS display_name TEXT NOT NULL DEFAULT '';
            "#,
        )
        .await
        .expect("conditional add column should be idempotent");

        let sqlite = pool.as_sqlite().expect("sqlite pool");
        let present = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('probe') WHERE name = 'display_name'",
        )
        .fetch_one(sqlite)
        .await
        .expect("column probe");
        assert_eq!(present, 1);
    }

    #[tokio::test]
    async fn sqlite_execute_sql_script_supports_multi_add_column_if_not_exists() {
        let config = sdkwork_database_config::DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };
        let pool = sdkwork_database_sqlx::create_pool_from_config(config)
            .await
            .expect("sqlite pool");

        execute_sql_script(
            &pool,
            r#"
            CREATE TABLE probe (id TEXT PRIMARY KEY);
            ALTER TABLE probe
              ADD COLUMN IF NOT EXISTS module_id TEXT NOT NULL DEFAULT 'legacy',
              ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';
            "#,
        )
        .await
        .expect("multi conditional add column should be supported");

        let sqlite = pool.as_sqlite().expect("sqlite pool");
        let present = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('probe') WHERE name IN ('module_id', 'status')",
        )
        .fetch_one(sqlite)
        .await
        .expect("column probe");
        assert_eq!(present, 2);
    }

    #[tokio::test]
    async fn sqlite_atomic_sql_script_supports_add_column_if_not_exists() {
        let config = sdkwork_database_config::DatabaseConfig {
            engine: DatabaseEngine::Sqlite,
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            ..Default::default()
        };
        let pool = sdkwork_database_sqlx::create_pool_from_config(config)
            .await
            .expect("sqlite pool");

        execute_sql_script_atomically(
            &pool,
            r#"
            CREATE TABLE probe (id TEXT PRIMARY KEY);
            ALTER TABLE probe ADD COLUMN IF NOT EXISTS display_name TEXT NOT NULL DEFAULT '';
            ALTER TABLE probe ADD COLUMN IF NOT EXISTS display_name TEXT NOT NULL DEFAULT '';
            "#,
        )
        .await
        .expect("atomic conditional add column should be idempotent");

        let sqlite = pool.as_sqlite().expect("sqlite pool");
        let present = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('probe') WHERE name = 'display_name'",
        )
        .fetch_one(sqlite)
        .await
        .expect("column probe");
        assert_eq!(present, 1);
    }

    #[test]
    fn test_engine_name_str() {
        assert_eq!(engine_name_str(DatabaseEngine::Sqlite), "sqlite");
        assert_eq!(engine_name_str(DatabaseEngine::Postgres), "postgres");
    }
}
