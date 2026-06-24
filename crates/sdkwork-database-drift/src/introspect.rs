use std::collections::BTreeMap;

use sdkwork_database_config::claw_database::resolve_unified_postgres_schema;
use sdkwork_database_config::DatabaseEngine;
use sdkwork_database_sqlx::DatabasePool;

use crate::error::DriftError;

fn postgres_application_schema() -> String {
    resolve_unified_postgres_schema("SDKWORK_CLAW")
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

pub async fn introspect_table_columns(
    pool: &DatabasePool,
) -> Result<BTreeMap<String, Vec<String>>, DriftError> {
    Ok(introspect_table_column_details(pool)
        .await?
        .into_iter()
        .map(|(table, columns)| {
            (
                table,
                columns.into_iter().map(|column| column.name).collect(),
            )
        })
        .collect())
}

pub async fn introspect_table_column_details(
    pool: &DatabasePool,
) -> Result<BTreeMap<String, Vec<ColumnInfo>>, DriftError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let tables = sqlx::query_scalar::<_, String>(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'ops_%' ORDER BY name",
            )
            .fetch_all(sqlite_pool)
            .await
            .map_err(|error| DriftError::Introspect(error.to_string()))?;

            let mut result = BTreeMap::new();
            for table in tables {
                let rows = sqlx::query_as::<_, (String, String, i64)>(&format!(
                    "SELECT name, type, \"notnull\" FROM pragma_table_info('{table}') ORDER BY cid"
                ))
                .fetch_all(sqlite_pool)
                .await
                .map_err(|error| DriftError::Introspect(error.to_string()))?;
                result.insert(
                    table,
                    rows.into_iter()
                        .map(|(name, data_type, notnull)| ColumnInfo {
                            name,
                            data_type,
                            nullable: notnull == 0,
                        })
                        .collect(),
                );
            }
            Ok(result)
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let schema = postgres_application_schema();
            let rows = sqlx::query_as::<_, (String, String, String, String)>(
                "SELECT table_name, column_name, data_type, is_nullable FROM information_schema.columns WHERE table_schema = $1 ORDER BY table_name, ordinal_position",
            )
            .bind(&schema)
            .fetch_all(pg_pool)
            .await
            .map_err(|error| DriftError::Introspect(error.to_string()))?;

            let mut result = BTreeMap::<String, Vec<ColumnInfo>>::new();
            for (table_name, column_name, data_type, is_nullable) in rows {
                result.entry(table_name).or_default().push(ColumnInfo {
                    name: column_name,
                    data_type,
                    nullable: is_nullable.eq_ignore_ascii_case("YES"),
                });
            }
            Ok(result)
        }
    }
}

pub async fn introspect_table_indexes(
    pool: &DatabasePool,
) -> Result<BTreeMap<String, Vec<String>>, DriftError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let tables = sqlx::query_scalar::<_, String>(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'ops_%' ORDER BY name",
            )
            .fetch_all(sqlite_pool)
            .await
            .map_err(|error| DriftError::Introspect(error.to_string()))?;

            let mut result = BTreeMap::new();
            for table in tables {
                let indexes = sqlx::query_scalar::<_, String>(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = $1 AND name NOT LIKE 'sqlite_%'",
                )
                .bind(&table)
                .fetch_all(sqlite_pool)
                .await
                .map_err(|error| DriftError::Introspect(error.to_string()))?;
                if !indexes.is_empty() {
                    result.insert(table, indexes);
                }
            }
            Ok(result)
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let schema = postgres_application_schema();
            let rows = sqlx::query_as::<_, (String, String)>(
                "SELECT tablename, indexname FROM pg_indexes WHERE schemaname = $1 ORDER BY tablename, indexname",
            )
            .bind(&schema)
            .fetch_all(pg_pool)
            .await
            .map_err(|error| DriftError::Introspect(error.to_string()))?;

            let mut result = BTreeMap::<String, Vec<String>>::new();
            for (table_name, index_name) in rows {
                if index_name.ends_with("_pkey") {
                    continue;
                }
                result.entry(table_name).or_default().push(index_name);
            }
            Ok(result)
        }
    }
}

pub async fn introspect_table_constraints(
    pool: &DatabasePool,
) -> Result<BTreeMap<String, Vec<String>>, DriftError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let tables = sqlx::query_scalar::<_, String>(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'ops_%' ORDER BY name",
            )
            .fetch_all(sqlite_pool)
            .await
            .map_err(|error| DriftError::Introspect(error.to_string()))?;

            let mut result = BTreeMap::new();
            for table in tables {
                let rows = sqlx::query_as::<_, (i64, String, i64, String, i64)>(&format!(
                    "SELECT seq, name, \"unique\", origin, partial FROM pragma_index_list('{table}') ORDER BY seq"
                ))
                .fetch_all(sqlite_pool)
                .await
                .map_err(|error| DriftError::Introspect(error.to_string()))?;

                let mut constraints = Vec::new();
                for (_, name, unique, _, _) in rows {
                    if unique == 0 || name.starts_with("sqlite_autoindex") {
                        continue;
                    }
                    constraints.push(name);
                }
                if !constraints.is_empty() {
                    result.insert(table, constraints);
                }
            }
            Ok(result)
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let schema = postgres_application_schema();
            let rows = sqlx::query_as::<_, (String, String)>(
                "SELECT table_name, constraint_name FROM information_schema.table_constraints WHERE table_schema = $1 AND constraint_type IN ('PRIMARY KEY', 'UNIQUE', 'FOREIGN KEY', 'CHECK') ORDER BY table_name, constraint_name",
            )
            .bind(&schema)
            .fetch_all(pg_pool)
            .await
            .map_err(|error| DriftError::Introspect(error.to_string()))?;

            let mut result = BTreeMap::<String, Vec<String>>::new();
            for (table_name, constraint_name) in rows {
                result.entry(table_name).or_default().push(constraint_name);
            }
            Ok(result)
        }
    }
}

pub async fn introspect_tables(pool: &DatabasePool) -> Result<Vec<String>, DriftError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let rows = sqlx::query_scalar::<_, String>(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .fetch_all(sqlite_pool)
            .await
            .map_err(|error| DriftError::Introspect(error.to_string()))?;
            Ok(rows)
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let schema = postgres_application_schema();
            let rows = sqlx::query_scalar::<_, String>(
                "SELECT table_name FROM information_schema.tables WHERE table_schema = $1 AND table_type = 'BASE TABLE' ORDER BY table_name",
            )
            .bind(&schema)
            .fetch_all(pg_pool)
            .await
            .map_err(|error| DriftError::Introspect(error.to_string()))?;
            Ok(rows)
        }
    }
}

pub fn engine_name(engine: DatabaseEngine) -> String {
    match engine {
        DatabaseEngine::Postgres => "postgres".to_string(),
        DatabaseEngine::Sqlite => "sqlite".to_string(),
    }
}
