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
    pub sqlite_rowid_alias: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexInfo {
    pub name: String,
    pub unique: bool,
    pub columns: Vec<String>,
    pub predicate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintInfo {
    pub name: Option<String>,
    pub constraint_type: String,
    pub columns: Vec<String>,
    pub references_table: Option<String>,
    pub references_columns: Vec<String>,
}

async fn sqlite_user_tables(pool: &sqlx::SqlitePool) -> Result<Vec<String>, DriftError> {
    sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| DriftError::Introspect(format!("sqlite table list: {error}")))
}

/// Introspect table columns (names only) from the live database.
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

/// Introspect full table column details (name, type, nullability) from the live database.
pub async fn introspect_table_column_details(
    pool: &DatabasePool,
) -> Result<BTreeMap<String, Vec<ColumnInfo>>, DriftError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let tables = sqlite_user_tables(sqlite_pool).await?;

            let mut result = BTreeMap::new();
            for table in tables {
                // Use parameterized query for PRAGMA. SQLite's pragma_table_info
                // supports parameterized table names via ?1 binding.
                let rows = sqlx::query_as::<_, (String, String, bool, i64)>(
                    "SELECT name, type, \"notnull\", pk FROM pragma_table_info(?) ORDER BY cid",
                )
                .bind(&table)
                .fetch_all(sqlite_pool)
                .await
                .map_err(|error| {
                    DriftError::Introspect(format!("sqlite pragma_table_info: {error}"))
                })?;

                let primary_key_column_count = rows.iter().filter(|(_, _, _, pk)| *pk > 0).count();
                let has_primary_key_index = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM pragma_index_list(?) WHERE origin = 'pk')",
                )
                .bind(&table)
                .fetch_one(sqlite_pool)
                .await
                .map_err(|error| {
                    DriftError::Introspect(format!("sqlite primary key index info: {error}"))
                })?;

                let columns = rows
                    .into_iter()
                    .map(
                        |(name, data_type, notnull, primary_key_ordinal)| ColumnInfo {
                            sqlite_rowid_alias: primary_key_column_count == 1
                                && primary_key_ordinal > 0
                                && data_type.trim().eq_ignore_ascii_case("INTEGER")
                                && !has_primary_key_index,
                            name,
                            data_type,
                            nullable: !notnull,
                        },
                    )
                    .collect();
                result.insert(table, columns);
            }
            Ok(result)
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let schema = postgres_application_schema();
            let rows = sqlx::query_as::<_, (String, String, String, String)>(
                "SELECT table_name, column_name, data_type, is_nullable \
                 FROM information_schema.columns \
                 WHERE table_schema = $1 \
                 ORDER BY table_name, ordinal_position",
            )
            .bind(&schema)
            .fetch_all(pg_pool)
            .await
            .map_err(|error| DriftError::Introspect(format!("postgres columns: {error}")))?;

            let mut result = BTreeMap::<String, Vec<ColumnInfo>>::new();
            for (table_name, column_name, data_type, is_nullable) in rows {
                result.entry(table_name).or_default().push(ColumnInfo {
                    name: column_name,
                    data_type,
                    nullable: is_nullable.eq_ignore_ascii_case("YES"),
                    sqlite_rowid_alias: false,
                });
            }
            Ok(result)
        }
    }
}

/// Introspect table indexes by name from the live database.
pub async fn introspect_table_indexes(
    pool: &DatabasePool,
) -> Result<BTreeMap<String, Vec<String>>, DriftError> {
    Ok(introspect_table_index_details(pool)
        .await?
        .into_iter()
        .filter_map(|(table, indexes)| {
            let names = indexes
                .into_iter()
                .filter(|index| {
                    !index.name.starts_with("sqlite_autoindex") && !index.name.ends_with("_pkey")
                })
                .map(|index| index.name)
                .collect::<Vec<_>>();
            (!names.is_empty()).then_some((table, names))
        })
        .collect())
}

/// Introspect table index details, including uniqueness and indexed columns.
pub async fn introspect_table_index_details(
    pool: &DatabasePool,
) -> Result<BTreeMap<String, Vec<IndexInfo>>, DriftError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let tables = sqlite_user_tables(sqlite_pool).await?;

            let mut result = BTreeMap::new();
            for table in tables {
                let rows = sqlx::query_as::<_, (String, bool, bool, Option<String>)>(
                    "SELECT index_metadata.name, index_metadata.\"unique\", \
                            index_metadata.partial, schema_metadata.sql \
                     FROM pragma_index_list(?) AS index_metadata \
                     LEFT JOIN sqlite_schema AS schema_metadata \
                       ON schema_metadata.type = 'index' \
                      AND schema_metadata.name = index_metadata.name \
                     ORDER BY index_metadata.seq",
                )
                .bind(&table)
                .fetch_all(sqlite_pool)
                .await
                .map_err(|error| DriftError::Introspect(format!("sqlite index list: {error}")))?;

                let mut indexes = Vec::with_capacity(rows.len());
                for (name, unique, partial, create_sql) in rows {
                    let columns = sqlx::query_scalar::<_, Option<String>>(
                        "SELECT name FROM pragma_index_info(?) ORDER BY seqno",
                    )
                    .bind(&name)
                    .fetch_all(sqlite_pool)
                    .await
                    .map_err(|error| DriftError::Introspect(format!("sqlite index info: {error}")))?
                    .into_iter()
                    .flatten()
                    .collect();
                    let predicate = if partial {
                        let create_sql = create_sql.as_deref().ok_or_else(|| {
                            DriftError::Introspect(format!(
                                "sqlite partial index definition is unavailable: {table}.{name}"
                            ))
                        })?;
                        Some(extract_sqlite_index_predicate(create_sql).ok_or_else(|| {
                            DriftError::Introspect(format!(
                                "sqlite partial index predicate is unavailable: {table}.{name}"
                            ))
                        })?)
                    } else {
                        None
                    };
                    indexes.push(IndexInfo {
                        name,
                        unique,
                        columns,
                        predicate,
                    });
                }
                if !indexes.is_empty() {
                    result.insert(table, indexes);
                }
            }
            Ok(result)
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let schema = postgres_application_schema();
            let rows = sqlx::query_as::<_, (String, String, bool, Vec<String>, Option<String>)>(
                "SELECT table_relation.relname, index_relation.relname, \
                        index_definition.indisunique, \
                        ARRAY( \
                            SELECT pg_get_indexdef( \
                                index_definition.indexrelid, key_position, TRUE \
                            ) \
                            FROM generate_series( \
                                1, index_definition.indnkeyatts::integer \
                            ) AS key_position \
                            ORDER BY key_position \
                        ), \
                        pg_get_expr( \
                            index_definition.indpred, index_definition.indrelid \
                        ) \
                 FROM pg_class AS table_relation \
                 JOIN pg_namespace AS namespace \
                   ON namespace.oid = table_relation.relnamespace \
                 JOIN pg_index AS index_definition \
                   ON index_definition.indrelid = table_relation.oid \
                 JOIN pg_class AS index_relation \
                   ON index_relation.oid = index_definition.indexrelid \
                 WHERE namespace.nspname = $1 \
                   AND table_relation.relkind IN ('r', 'p') \
                 ORDER BY table_relation.relname, index_relation.relname",
            )
            .bind(&schema)
            .fetch_all(pg_pool)
            .await
            .map_err(|error| DriftError::Introspect(format!("postgres indexes: {error}")))?;

            let mut result = BTreeMap::<String, Vec<IndexInfo>>::new();
            for (table_name, index_name, unique, columns, predicate) in rows {
                result.entry(table_name).or_default().push(IndexInfo {
                    name: index_name,
                    unique,
                    columns,
                    predicate,
                });
            }
            Ok(result)
        }
    }
}

fn extract_sqlite_index_predicate(create_sql: &str) -> Option<String> {
    let bytes = create_sql.as_bytes();
    let mut index = 0;
    let mut depth = 0_u32;
    let mut quote = None;

    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(terminator) = quote {
            if byte == terminator {
                if terminator != b']' && bytes.get(index + 1) == Some(&terminator) {
                    index += 2;
                    continue;
                }
                quote = None;
            }
            index += 1;
            continue;
        }

        match byte {
            b'\'' | b'"' | b'`' => quote = Some(byte),
            b'[' => quote = Some(b']'),
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ if depth == 0 && byte.is_ascii_alphabetic() => {
                let start = index;
                while bytes
                    .get(index)
                    .is_some_and(|value| value.is_ascii_alphanumeric() || *value == b'_')
                {
                    index += 1;
                }
                if create_sql[start..index].eq_ignore_ascii_case("where") {
                    let predicate = create_sql[index..].trim().trim_end_matches(';').trim();
                    return (!predicate.is_empty()).then(|| predicate.to_string());
                }
                continue;
            }
            _ => {}
        }
        index += 1;
    }

    None
}

/// Introspect table constraints from the live database.
pub async fn introspect_table_constraints(
    pool: &DatabasePool,
) -> Result<BTreeMap<String, Vec<String>>, DriftError> {
    Ok(introspect_table_constraint_details(pool)
        .await?
        .into_iter()
        .filter_map(|(table, constraints)| {
            let names = constraints
                .into_iter()
                .filter_map(|constraint| constraint.name)
                .filter(|name| !name.starts_with("sqlite_autoindex"))
                .collect::<Vec<_>>();
            (!names.is_empty()).then_some((table, names))
        })
        .collect())
}

/// Introspect constraints that the database exposes through structured metadata.
pub async fn introspect_table_constraint_details(
    pool: &DatabasePool,
) -> Result<BTreeMap<String, Vec<ConstraintInfo>>, DriftError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => {
            let tables = sqlite_user_tables(sqlite_pool).await?;
            let indexes = introspect_table_index_details(pool).await?;

            let mut result = BTreeMap::new();
            for table in tables {
                let mut constraints = Vec::new();

                let primary_key_columns = sqlx::query_as::<_, (String, i64)>(
                    "SELECT name, pk FROM pragma_table_info(?) WHERE pk > 0 ORDER BY pk",
                )
                .bind(&table)
                .fetch_all(sqlite_pool)
                .await
                .map_err(|error| {
                    DriftError::Introspect(format!("sqlite primary key info: {error}"))
                })?
                .into_iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>();
                if !primary_key_columns.is_empty() {
                    constraints.push(ConstraintInfo {
                        name: None,
                        constraint_type: "primary_key".to_string(),
                        columns: primary_key_columns,
                        references_table: None,
                        references_columns: Vec::new(),
                    });
                }

                if let Some(table_indexes) = indexes.get(&table) {
                    constraints.extend(table_indexes.iter().filter(|index| index.unique).map(
                        |index| ConstraintInfo {
                            name: Some(index.name.clone()),
                            constraint_type: "unique".to_string(),
                            columns: index.columns.clone(),
                            references_table: None,
                            references_columns: Vec::new(),
                        },
                    ));
                }

                let foreign_key_rows =
                    sqlx::query_as::<_, (i64, i64, String, String, Option<String>)>(
                        "SELECT id, seq, \"table\", \"from\", \"to\" \
                     FROM pragma_foreign_key_list(?) ORDER BY id, seq",
                    )
                    .bind(&table)
                    .fetch_all(sqlite_pool)
                    .await
                    .map_err(|error| {
                        DriftError::Introspect(format!("sqlite foreign key info: {error}"))
                    })?;
                let mut foreign_keys = BTreeMap::<i64, ConstraintInfo>::new();
                for (id, _, references_table, column, references_column) in foreign_key_rows {
                    let foreign_key = foreign_keys.entry(id).or_insert_with(|| ConstraintInfo {
                        name: None,
                        constraint_type: "foreign_key".to_string(),
                        columns: Vec::new(),
                        references_table: Some(references_table),
                        references_columns: Vec::new(),
                    });
                    foreign_key.columns.push(column);
                    if let Some(references_column) = references_column {
                        foreign_key.references_columns.push(references_column);
                    }
                }
                constraints.extend(foreign_keys.into_values());

                if !constraints.is_empty() {
                    result.insert(table, constraints);
                }
            }
            Ok(result)
        }
        DatabasePool::Postgres(pg_pool, _) => {
            let schema = postgres_application_schema();
            let rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
                "SELECT constraint_info.table_name, \
                        constraint_info.constraint_name, \
                        constraint_info.constraint_type, \
                        key_column.column_name \
                 FROM information_schema.table_constraints AS constraint_info \
                 LEFT JOIN information_schema.key_column_usage AS key_column \
                   ON key_column.constraint_catalog = constraint_info.constraint_catalog \
                  AND key_column.constraint_schema = constraint_info.constraint_schema \
                  AND key_column.constraint_name = constraint_info.constraint_name \
                  AND key_column.table_schema = constraint_info.table_schema \
                  AND key_column.table_name = constraint_info.table_name \
                 WHERE constraint_info.table_schema = $1 \
                   AND constraint_info.constraint_type IN \
                       ('PRIMARY KEY', 'UNIQUE', 'FOREIGN KEY', 'CHECK') \
                 ORDER BY constraint_info.table_name, constraint_info.constraint_name, \
                          key_column.ordinal_position",
            )
            .bind(&schema)
            .fetch_all(pg_pool)
            .await
            .map_err(|error| DriftError::Introspect(format!("postgres constraints: {error}")))?;

            let mut grouped = BTreeMap::<(String, String), ConstraintInfo>::new();
            for (table_name, constraint_name, constraint_type, column_name) in rows {
                let constraint = grouped
                    .entry((table_name, constraint_name.clone()))
                    .or_insert_with(|| ConstraintInfo {
                        name: Some(constraint_name),
                        constraint_type: normalize_constraint_type(&constraint_type),
                        columns: Vec::new(),
                        references_table: None,
                        references_columns: Vec::new(),
                    });
                if let Some(column_name) = column_name {
                    constraint.columns.push(column_name);
                }
            }

            let mut result = BTreeMap::<String, Vec<ConstraintInfo>>::new();
            for ((table_name, _), constraint) in grouped {
                result.entry(table_name).or_default().push(constraint);
            }
            Ok(result)
        }
    }
}

fn normalize_constraint_type(constraint_type: &str) -> String {
    constraint_type
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "_")
}

/// Introspect all user tables from the live database (names only).
pub async fn introspect_tables(pool: &DatabasePool) -> Result<Vec<String>, DriftError> {
    match pool {
        DatabasePool::Sqlite(sqlite_pool, _) => sqlite_user_tables(sqlite_pool).await,
        DatabasePool::Postgres(pg_pool, _) => {
            let schema = postgres_application_schema();
            let rows = sqlx::query_scalar::<_, String>(
                "SELECT table_name \
                 FROM information_schema.tables \
                 WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
                 ORDER BY table_name",
            )
            .bind(&schema)
            .fetch_all(pg_pool)
            .await
            .map_err(|error| DriftError::Introspect(format!("postgres tables: {error}")))?;
            Ok(rows)
        }
    }
}

/// Convert DatabaseEngine to a human-readable engine name string.
pub fn engine_name(engine: DatabaseEngine) -> String {
    match engine {
        DatabaseEngine::Postgres => "postgres".to_string(),
        DatabaseEngine::Sqlite => "sqlite".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_name() {
        assert_eq!(engine_name(DatabaseEngine::Sqlite), "sqlite");
        assert_eq!(engine_name(DatabaseEngine::Postgres), "postgres");
    }

    #[test]
    fn sqlite_partial_index_predicate_ignores_where_inside_expressions() {
        let sql = "CREATE INDEX idx_probe ON probe(lower('where value'), id) \
                   WHERE deleted_at IS NULL;";
        assert_eq!(
            extract_sqlite_index_predicate(sql).as_deref(),
            Some("deleted_at IS NULL")
        );
    }
}
