use sdkwork_database_config::DatabaseEngine;
use sdkwork_database_sqlx::DatabasePool;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::error::HistoryError;

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

pub async fn ensure_history_tables(pool: &DatabasePool) -> Result<(), HistoryError> {
    let sql = match pool.engine() {
        DatabaseEngine::Postgres => HISTORY_TABLES_SQL_POSTGRES,
        DatabaseEngine::Sqlite => HISTORY_TABLES_SQL_SQLITE,
    };
    execute_sql_script(pool, sql).await?;
    Ok(())
}

pub async fn execute_sql_script(pool: &DatabasePool, script: &str) -> Result<(), HistoryError> {
    for statement in split_sql_statements(script) {
        if statement.is_empty() {
            continue;
        }
        execute_sql(pool, &statement).await?;
    }
    Ok(())
}

pub async fn execute_sql(pool: &DatabasePool, sql: &str) -> Result<(), HistoryError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            sqlx::raw_sql(sql)
                .execute(sqlite_pool)
                .await
                .map_err(|error| HistoryError::Sql(format!("sqlite execute failed: {error}")))?;
        }
        DatabasePool::Postgres(pg_pool, _) => {
            sqlx::raw_sql(sql)
                .execute(pg_pool)
                .await
                .map_err(|error| HistoryError::Sql(format!("postgres execute failed: {error}")))?;
        }
    }
    Ok(())
}

pub fn file_checksum(path: &std::path::Path) -> Result<String, HistoryError> {
    let bytes = std::fs::read(path).map_err(|error| {
        HistoryError::State(format!("failed to read {}: {error}", path.display()))
    })?;
    let digest = Sha256::digest(bytes);
    Ok(hex::encode(digest))
}

pub async fn list_applied_migration_versions(
    pool: &DatabasePool,
    module_id: &str,
    engine: DatabaseEngine,
) -> Result<Vec<String>, HistoryError> {
    let engine_name = engine_name(engine);
    let query =
        "SELECT version FROM ops_schema_migration_history WHERE module_id = $1 AND engine = $2 ORDER BY version";
    fetch_version_column(pool, query, module_id, engine_name).await
}

pub async fn migration_checksum(
    pool: &DatabasePool,
    module_id: &str,
    version: &str,
    engine: DatabaseEngine,
) -> Result<Option<String>, HistoryError> {
    let engine_name = engine_name(engine);
    let query = r#"
        SELECT checksum FROM ops_schema_migration_history
        WHERE module_id = $1 AND version = $2 AND engine = $3
    "#;
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let row = sqlx::query(query)
                .bind(module_id)
                .bind(version)
                .bind(engine_name)
                .fetch_optional(sqlite_pool)
                .await
                .map_err(|error| HistoryError::Sql(error.to_string()))?;
            Ok(row.map(|row| row.get::<String, _>("checksum")))
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let row = sqlx::query(query)
                .bind(module_id)
                .bind(version)
                .bind(engine_name)
                .fetch_optional(pg_pool)
                .await
                .map_err(|error| HistoryError::Sql(error.to_string()))?;
            Ok(row.map(|row| row.get::<String, _>("checksum")))
        }
    }
}

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
    let engine_name = engine_name(engine);
    let query = r#"
        INSERT INTO ops_schema_migration_history
            (module_id, version, name, engine, checksum, applied_by, execution_ms)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
    "#;
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            sqlx::query(query)
                .bind(module_id)
                .bind(version)
                .bind(name)
                .bind(engine_name)
                .bind(checksum)
                .bind(applied_by)
                .bind(execution_ms)
                .execute(sqlite_pool)
                .await
                .map_err(|error| HistoryError::Migration(error.to_string()))?;
        }
        DatabasePool::Postgres(pg_pool, _) => {
            sqlx::query(query)
                .bind(module_id)
                .bind(version)
                .bind(name)
                .bind(engine_name)
                .bind(checksum)
                .bind(applied_by)
                .bind(execution_ms)
                .execute(pg_pool)
                .await
                .map_err(|error| HistoryError::Migration(error.to_string()))?;
        }
    }
    Ok(())
}

pub async fn is_seed_applied(
    pool: &DatabasePool,
    module_id: &str,
    seed_id: &str,
    locale: &str,
    profile: &str,
) -> Result<bool, HistoryError> {
    let query = r#"
        SELECT 1 as found FROM ops_seed_history
        WHERE module_id = $1 AND seed_id = $2 AND locale = $3 AND profile = $4
        LIMIT 1
    "#;
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let row = sqlx::query(query)
                .bind(module_id)
                .bind(seed_id)
                .bind(locale)
                .bind(profile)
                .fetch_optional(sqlite_pool)
                .await
                .map_err(|error| HistoryError::Seed(error.to_string()))?;
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
                .map_err(|error| HistoryError::Seed(error.to_string()))?;
            Ok(row.is_some())
        }
    }
}

pub async fn record_seed(
    pool: &DatabasePool,
    module_id: &str,
    seed_id: &str,
    locale: &str,
    profile: &str,
    checksum: &str,
    applied_by: &str,
) -> Result<(), HistoryError> {
    let query = r#"
        INSERT INTO ops_seed_history
            (module_id, seed_id, locale, profile, checksum, applied_by)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT(module_id, seed_id, locale, profile) DO UPDATE SET
            checksum = excluded.checksum,
            applied_by = excluded.applied_by
    "#;
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
                .map_err(|error| HistoryError::Seed(error.to_string()))?;
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
                .map_err(|error| HistoryError::Seed(error.to_string()))?;
        }
    }
    Ok(())
}

pub async fn upsert_installation_state(
    pool: &DatabasePool,
    module_id: &str,
    contract_version: &str,
    seed_locale: &str,
    seed_profile: &str,
    status: &str,
) -> Result<(), HistoryError> {
    let query = r#"
        INSERT INTO ops_database_installation_state
            (id, module_id, contract_version, seed_locale, seed_profile, status)
        VALUES (1, $1, $2, $3, $4, $5)
        ON CONFLICT(id) DO UPDATE SET
            module_id = excluded.module_id,
            contract_version = excluded.contract_version,
            seed_locale = excluded.seed_locale,
            seed_profile = excluded.seed_profile,
            status = excluded.status
    "#;
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
                .map_err(|error| HistoryError::State(error.to_string()))?;
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
                .map_err(|error| HistoryError::State(error.to_string()))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct InstallationState {
    pub module_id: String,
    pub contract_version: Option<String>,
    pub seed_locale: Option<String>,
    pub seed_profile: Option<String>,
    pub status: String,
}

pub async fn fetch_installation_state(
    pool: &DatabasePool,
) -> Result<Option<InstallationState>, HistoryError> {
    let query = r#"
        SELECT module_id, contract_version, seed_locale, seed_profile, status
        FROM ops_database_installation_state
        WHERE id = 1
        LIMIT 1
    "#;
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let row = sqlx::query(query)
                .fetch_optional(sqlite_pool)
                .await
                .map_err(|error| HistoryError::State(error.to_string()))?;
            Ok(row.map(|row| InstallationState {
                module_id: row.get("module_id"),
                contract_version: row.get("contract_version"),
                seed_locale: row.get("seed_locale"),
                seed_profile: row.get("seed_profile"),
                status: row.get("status"),
            }))
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let row = sqlx::query(query)
                .fetch_optional(pg_pool)
                .await
                .map_err(|error| HistoryError::State(error.to_string()))?;
            Ok(row.map(|row| InstallationState {
                module_id: row.get("module_id"),
                contract_version: row.get("contract_version"),
                seed_locale: row.get("seed_locale"),
                seed_profile: row.get("seed_profile"),
                status: row.get("status"),
            }))
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppliedSeedRecord {
    pub seed_id: String,
    pub locale: String,
    pub profile: String,
    pub checksum: String,
}

pub async fn list_applied_seeds(
    pool: &DatabasePool,
    module_id: &str,
) -> Result<Vec<AppliedSeedRecord>, HistoryError> {
    let query = r#"
        SELECT seed_id, locale, profile, checksum
        FROM ops_seed_history
        WHERE module_id = $1
        ORDER BY seed_id, locale, profile
    "#;
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let rows = sqlx::query(query)
                .bind(module_id)
                .fetch_all(sqlite_pool)
                .await
                .map_err(|error| HistoryError::Seed(error.to_string()))?;
            Ok(rows
                .iter()
                .map(|row| AppliedSeedRecord {
                    seed_id: row.get("seed_id"),
                    locale: row.get("locale"),
                    profile: row.get("profile"),
                    checksum: row.get("checksum"),
                })
                .collect())
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let rows = sqlx::query(query)
                .bind(module_id)
                .fetch_all(pg_pool)
                .await
                .map_err(|error| HistoryError::Seed(error.to_string()))?;
            Ok(rows
                .iter()
                .map(|row| AppliedSeedRecord {
                    seed_id: row.get("seed_id"),
                    locale: row.get("locale"),
                    profile: row.get("profile"),
                    checksum: row.get("checksum"),
                })
                .collect())
        }
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
                .map_err(|error| HistoryError::Sql(error.to_string()))?;
            Ok(rows
                .iter()
                .map(|row| row.get::<String, _>("version"))
                .collect())
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let rows = sqlx::query(query)
                .bind(module_id)
                .bind(engine_name)
                .fetch_all(pg_pool)
                .await
                .map_err(|error| HistoryError::Sql(error.to_string()))?;
            Ok(rows
                .iter()
                .map(|row| row.get::<String, _>("version"))
                .collect())
        }
    }
}

fn engine_name(engine: DatabaseEngine) -> &'static str {
    match engine {
        DatabaseEngine::Postgres => "postgres",
        DatabaseEngine::Sqlite => "sqlite",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SqlSplitState {
    Normal,
    SingleQuoted,
    DoubleQuoted,
    DollarQuoted,
    LineComment,
    BlockComment,
}

fn split_sql_statements(script: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut state = SqlSplitState::Normal;
    let mut dollar_tag: Option<String> = None;
    let bytes = script.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let ch = bytes[index] as char;

        match state {
            SqlSplitState::Normal => {
                if ch == '-' && next_char(bytes, index) == Some('-') {
                    current.push(ch);
                    current.push('-');
                    index += 2;
                    state = SqlSplitState::LineComment;
                    continue;
                }
                if ch == '/' && next_char(bytes, index) == Some('*') {
                    current.push(ch);
                    current.push('*');
                    index += 2;
                    state = SqlSplitState::BlockComment;
                    continue;
                }
                if ch == '\'' {
                    current.push(ch);
                    index += 1;
                    state = SqlSplitState::SingleQuoted;
                    continue;
                }
                if ch == '"' {
                    current.push(ch);
                    index += 1;
                    state = SqlSplitState::DoubleQuoted;
                    continue;
                }
                if ch == '$' {
                    let (tag, next_index) = read_dollar_tag(script, index);
                    if tag.ends_with('$') && tag.len() >= 2 {
                        current.push_str(&tag);
                        dollar_tag = Some(tag);
                        index = next_index;
                        state = SqlSplitState::DollarQuoted;
                        continue;
                    }
                    current.push(ch);
                    index += 1;
                    continue;
                }
                if ch == ';' {
                    push_trimmed_statement(&mut statements, &current);
                    current.clear();
                    index += 1;
                    continue;
                }
                current.push(ch);
                index += 1;
            }
            SqlSplitState::SingleQuoted => {
                if ch == '\'' {
                    if next_char(bytes, index) == Some('\'') {
                        current.push('\'');
                        current.push('\'');
                        index += 2;
                    } else {
                        current.push('\'');
                        index += 1;
                        state = SqlSplitState::Normal;
                    }
                } else {
                    current.push(ch);
                    index += 1;
                }
            }
            SqlSplitState::DoubleQuoted => {
                if ch == '"' {
                    if next_char(bytes, index) == Some('"') {
                        current.push('"');
                        current.push('"');
                        index += 2;
                    } else {
                        current.push('"');
                        index += 1;
                        state = SqlSplitState::Normal;
                    }
                } else {
                    current.push(ch);
                    index += 1;
                }
            }
            SqlSplitState::DollarQuoted => {
                if ch == '$' {
                    let (closing, next_index) = read_dollar_tag(script, index);
                    current.push_str(&closing);
                    index = next_index;
                    if dollar_tag.as_deref() == Some(closing.as_str()) {
                        dollar_tag = None;
                        state = SqlSplitState::Normal;
                    }
                    continue;
                }
                current.push(ch);
                index += 1;
            }
            SqlSplitState::LineComment => {
                current.push(ch);
                index += 1;
                if ch == '\n' {
                    state = SqlSplitState::Normal;
                }
            }
            SqlSplitState::BlockComment => {
                if ch == '*' && next_char(bytes, index) == Some('/') {
                    current.push('*');
                    current.push('/');
                    index += 2;
                    state = SqlSplitState::Normal;
                } else {
                    current.push(ch);
                    index += 1;
                }
            }
        }
    }

    push_trimmed_statement(&mut statements, &current);
    statements
}

fn next_char(bytes: &[u8], index: usize) -> Option<char> {
    bytes.get(index + 1).map(|byte| *byte as char)
}

fn read_dollar_tag(script: &str, start: usize) -> (String, usize) {
    let bytes = script.as_bytes();
    let mut index = start + 1;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '$' {
            index += 1;
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' {
            index += 1;
        } else {
            break;
        }
    }
    (script[start..index].to_string(), index)
}

fn push_trimmed_statement(statements: &mut Vec<String>, current: &str) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(format!("{trimmed};"));
    }
}

#[cfg(test)]
mod split_sql_statements_tests {
    use super::split_sql_statements;

    #[test]
    fn splits_simple_statements() {
        let statements = split_sql_statements("SELECT 1; SELECT 2;");
        assert_eq!(statements, vec!["SELECT 1;", "SELECT 2;"]);
    }

    #[test]
    fn keeps_semicolons_inside_single_quoted_strings() {
        let statements = split_sql_statements("SELECT 'a;b'; SELECT 2;");
        assert_eq!(statements, vec!["SELECT 'a;b';", "SELECT 2;"]);
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
        let statements = split_sql_statements(script);
        assert_eq!(statements.len(), 2);
        assert!(statements[0].contains("DO $$"));
        assert!(statements[0].contains("END $$;"));
        assert_eq!(statements[1], "SELECT 1;");
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
        let statements = split_sql_statements(script);
        assert_eq!(statements.len(), 2);
        assert!(statements[0].contains("DO $body$"));
        assert_eq!(statements[1], "SELECT 1;");
    }

    #[test]
    fn keeps_semicolons_inside_dollar_quoted_blocks_with_crlf() {
        let script = "DO $$\r\nBEGIN\r\n    IF TRUE THEN\r\n        PERFORM 1;\r\n    END IF;\r\nEND $$;\r\nSELECT 1;\r\n";
        let statements = split_sql_statements(script);
        assert_eq!(statements.len(), 2);
        assert!(statements[0].starts_with("DO $$"));
        assert!(statements[0].contains("END $$;"));
        assert_eq!(statements[1], "SELECT 1;");
    }
}
