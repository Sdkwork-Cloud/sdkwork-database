use chrono::Utc;
use sdkwork_database_contract::{
    load_expected_column_required, load_expected_column_types, load_expected_columns,
    load_expected_constraints, load_expected_indexes, load_expected_tables,
};
use sdkwork_database_history::{
    file_checksum, list_applied_migration_versions, migration_checksum,
};
use sdkwork_database_spi::types::DriftPolicy;
use sdkwork_database_sqlx::DatabasePool;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::DriftError;
use crate::introspect::{
    engine_name, introspect_table_column_details, introspect_table_constraint_details,
    introspect_table_index_details, introspect_tables,
};
use crate::matching::{
    constraint_is_satisfied, index_is_satisfied, physical_type_matches_for_engine,
};

/// Known framework history tables that are excluded from drift extra-table detection.
const FRAMEWORK_HISTORY_TABLES: &[&str] = &[
    "ops_schema_migration_history",
    "ops_seed_history",
    "ops_database_installation_state",
];

/// Drift report — the top-level result of a drift analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub schema_version: u32,
    pub kind: String,
    pub checked_at: String,
    pub module_id: String,
    pub service_code: String,
    pub engine: String,
    pub status: String,
    pub summary: DriftSummary,
    pub pending_migrations: Vec<String>,
    pub live_tables: Vec<String>,
    pub expected_tables: Vec<String>,
    pub diffs: Vec<DriftDiff>,
}

/// Aggregated drift summary counts by severity.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DriftSummary {
    pub error: u32,
    pub warn: u32,
    pub info: u32,
}

/// A single drift difference entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftDiff {
    pub code: String,
    pub severity: String,
    pub message: String,
}

/// Engine that compares expected schema (contract) against live database schema.
pub struct DriftEngine {
    pool: DatabasePool,
    module: std::sync::Arc<dyn sdkwork_database_spi::DatabaseModule>,
}

impl DriftEngine {
    pub fn new(
        pool: DatabasePool,
        module: std::sync::Arc<dyn sdkwork_database_spi::DatabaseModule>,
    ) -> Self {
        Self { pool, module }
    }

    /// Run full drift analysis: migrations, checksums, tables, columns, types,
    /// nullability, indexes, and constraints.
    pub async fn analyze(&self) -> Result<DriftReport, DriftError> {
        let descriptor = self.module.descriptor();
        let engine = self.pool.engine();
        let policy = self.module.load_policy().await?;
        let contract_path = self.module.contract_path();
        let contract_exists = contract_path.exists();

        // ── Migration drift ──────────────────────────────────────────────
        let migrations = self.module.list_migrations(engine).await?;
        let applied =
            list_applied_migration_versions(&self.pool, &descriptor.module_id, engine).await?;

        let known_versions: Vec<String> = migrations.iter().map(|m| m.version.clone()).collect();

        let mut pending_migrations = Vec::new();
        let mut diffs = Vec::new();

        for migration in &migrations {
            let id = format!("{}_{}", migration.version, migration.name);
            if !applied.contains(&migration.version) {
                pending_migrations.push(id.clone());
                diffs.push(DriftDiff {
                    code: "migration_pending".to_string(),
                    severity: severity_for("migration_pending", &policy),
                    message: format!("pending migration: {id}"),
                });
                continue;
            }

            let checksum = file_checksum(&migration.up_path)?;
            if let Some(existing) = migration_checksum(
                &self.pool,
                &descriptor.module_id,
                &migration.version,
                engine,
            )
            .await?
            {
                if existing != checksum {
                    diffs.push(DriftDiff {
                        code: "checksum_mismatch".to_string(),
                        severity: severity_for("checksum_mismatch", &policy),
                        message: format!(
                            "checksum_mismatch for migration {}: applied={}, current={}",
                            migration.version, existing, checksum
                        ),
                    });
                }
            }
        }

        for version in &applied {
            if !known_versions.iter().any(|v| v == version) {
                diffs.push(DriftDiff {
                    code: "migration_unknown".to_string(),
                    severity: severity_for("migration_unknown", &policy),
                    message: format!("migration_unknown: {version}"),
                });
            }
        }

        // ── Table drift ─────────────────────────────────────────────────
        let live_tables = introspect_tables(&self.pool).await?;
        let expected_tables = if contract_exists {
            // contract_path.parent() is guaranteed Some since contract_path always has a parent
            let registry_dir = contract_path.parent().unwrap();
            let table_registry_path = registry_dir.join("table-registry.json");
            load_expected_tables(&contract_path, &table_registry_path)?
        } else {
            Vec::new()
        };

        for expected in &expected_tables {
            if policy.ignore_tables.iter().any(|t| t == expected) {
                continue;
            }
            if !live_tables.iter().any(|t| t == expected) {
                diffs.push(DriftDiff {
                    code: "missing_table".to_string(),
                    severity: severity_for("missing_table", &policy),
                    message: format!("missing table: {expected}"),
                });
            }
        }

        for live in &live_tables {
            if is_framework_table(live) || policy.ignore_tables.iter().any(|t| t == live) {
                continue;
            }
            if !expected_tables.is_empty() && !expected_tables.iter().any(|t| t == live) {
                diffs.push(DriftDiff {
                    code: "extra_table".to_string(),
                    severity: severity_for("extra_table", &policy),
                    message: format!("extra table: {live}"),
                });
            }
        }

        // ── Column / type / nullability drift ───────────────────────────
        if contract_exists {
            let expected_columns = load_expected_columns(&contract_path)?;
            let expected_column_types = load_expected_column_types(&contract_path)?;
            let expected_column_required = load_expected_column_required(&contract_path)?;
            let expected_indexes = load_expected_indexes(&contract_path)?;
            let expected_constraints = load_expected_constraints(&contract_path)?;

            if !expected_columns.is_empty() || !expected_column_types.is_empty() {
                let live_column_details = introspect_table_column_details(&self.pool).await?;

                for (table_name, expected) in expected_columns {
                    if policy.ignore_tables.iter().any(|t| t == &table_name) {
                        continue;
                    }
                    let live = live_column_details
                        .get(&table_name)
                        .cloned()
                        .unwrap_or_default();
                    let live_names: Vec<String> = live.iter().map(|c| c.name.clone()).collect();

                    // missing columns
                    for column in &expected {
                        if policy.ignore_columns.iter().any(|c| c == column) {
                            continue;
                        }
                        if !live_names.iter().any(|n| n == column) {
                            diffs.push(DriftDiff {
                                code: "missing_column".to_string(),
                                severity: severity_for("missing_column", &policy),
                                message: format!("missing column: {table_name}.{column}"),
                            });
                        }
                    }

                    // extra / type / nullability mismatches
                    for column in &live {
                        if policy.ignore_columns.iter().any(|c| c == &column.name) {
                            continue;
                        }
                        if !expected.iter().any(|n| n == &column.name) {
                            diffs.push(DriftDiff {
                                code: "extra_column".to_string(),
                                severity: severity_for("extra_column", &policy),
                                message: format!("extra column: {table_name}.{}", column.name),
                            });
                            continue;
                        }
                        // type mismatch
                        if let Some(expected_types) = expected_column_types.get(&table_name) {
                            if let Some(logical_type) = expected_types.get(&column.name) {
                                if !physical_type_matches_for_engine(
                                    engine,
                                    logical_type,
                                    &column.data_type,
                                ) {
                                    diffs.push(DriftDiff {
                                        code: "type_mismatch".to_string(),
                                        severity: severity_for("type_mismatch", &policy),
                                        message: format!(
                                            "type_mismatch: {table_name}.{} expected={logical_type} actual={}",
                                            column.name, column.data_type
                                        ),
                                    });
                                }
                                if column.sqlite_rowid_alias {
                                    diffs.push(DriftDiff {
                                        code: "sqlite_rowid_alias".to_string(),
                                        severity: severity_for("sqlite_rowid_alias", &policy),
                                        message: format!(
                                            "sqlite_rowid_alias: {table_name}.{} expected={logical_type} actual=INTEGER PRIMARY KEY; SDKWork business ids must be explicitly allocated and bound",
                                            column.name
                                        ),
                                    });
                                }
                            }
                        }
                        // nullability mismatch
                        if let Some(required_map) = expected_column_required.get(&table_name) {
                            if let Some(expected_required) = required_map.get(&column.name) {
                                let expected_nullable = !expected_required;
                                if column.nullable != expected_nullable {
                                    diffs.push(DriftDiff {
                                        code: "type_mismatch".to_string(),
                                        severity: severity_for("type_mismatch", &policy),
                                        message: format!(
                                            "nullability_mismatch: {table_name}.{} expected_nullable={expected_nullable} actual_nullable={}",
                                            column.name, column.nullable
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // ── Index drift ─────────────────────────────────────────────
            let live_index_details =
                if !expected_indexes.is_empty() || !expected_constraints.is_empty() {
                    introspect_table_index_details(&self.pool).await?
                } else {
                    BTreeMap::new()
                };

            if !expected_indexes.is_empty() || !expected_constraints.is_empty() {
                let expected_constraint_names: BTreeMap<String, Vec<String>> = expected_constraints
                    .iter()
                    .map(|(table, constraints)| {
                        (
                            table.clone(),
                            constraints.iter().map(|e| e.name.clone()).collect(),
                        )
                    })
                    .collect();

                for (table_name, indexes) in &expected_indexes {
                    if policy.ignore_tables.iter().any(|t| t == table_name) {
                        continue;
                    }
                    let live = live_index_details
                        .get(table_name)
                        .cloned()
                        .unwrap_or_default();
                    for index in indexes {
                        match live.iter().find(|live_index| live_index.name == index.name) {
                            None => diffs.push(DriftDiff {
                                code: "missing_index".to_string(),
                                severity: severity_for("missing_index", &policy),
                                message: format!("missing index: {table_name}.{}", index.name),
                            }),
                            Some(live_index) if !index_is_satisfied(index, live_index) => {
                                diffs.push(DriftDiff {
                                    code: "index_mismatch".to_string(),
                                    severity: severity_for("index_mismatch", &policy),
                                    message: format!(
                                        "index_mismatch: {table_name}.{} expected_columns={:?} actual_columns={:?} expected_unique={} actual_unique={} expected_predicate={:?} actual_predicate={:?}",
                                        index.name,
                                        index.columns,
                                        live_index.columns,
                                        index.unique,
                                        live_index.unique,
                                        index.predicate,
                                        live_index.predicate
                                    ),
                                });
                            }
                            Some(_) => {}
                        }
                    }
                }

                for (table_name, live_indexes) in &live_index_details {
                    if is_framework_table(table_name)
                        || policy.ignore_tables.iter().any(|t| t == table_name)
                    {
                        continue;
                    }
                    let expected_idx_names: Vec<String> = expected_indexes
                        .get(table_name)
                        .map(|idx| idx.iter().map(|i| i.name.clone()).collect())
                        .unwrap_or_default();
                    let expected_cons_names: Vec<String> = expected_constraint_names
                        .get(table_name)
                        .cloned()
                        .unwrap_or_default();

                    for live_index in live_indexes {
                        let index_name = &live_index.name;
                        if is_auto_index(index_name) {
                            continue;
                        }
                        if expected_idx_names.iter().any(|n| n == index_name)
                            || expected_cons_names.iter().any(|n| n == index_name)
                        {
                            continue;
                        }
                        diffs.push(DriftDiff {
                            code: "extra_index".to_string(),
                            severity: severity_for("extra_index", &policy),
                            message: format!("extra index: {table_name}.{}", live_index.name),
                        });
                    }
                }
            }

            // ── Constraint drift ────────────────────────────────────────
            if !expected_constraints.is_empty() {
                let live_constraints = introspect_table_constraint_details(&self.pool).await?;
                for (table_name, constraints) in &expected_constraints {
                    if policy.ignore_tables.iter().any(|t| t == table_name) {
                        continue;
                    }
                    let live = live_constraints
                        .get(table_name)
                        .cloned()
                        .unwrap_or_default();
                    let live_indexes = live_index_details
                        .get(table_name)
                        .cloned()
                        .unwrap_or_default();
                    for constraint in constraints {
                        if !constraint_is_satisfied(engine, constraint, &live, &live_indexes) {
                            diffs.push(DriftDiff {
                                code: "missing_constraint".to_string(),
                                severity: severity_for("missing_constraint", &policy),
                                message: format!(
                                    "missing constraint: {table_name}.{}",
                                    constraint.name
                                ),
                            });
                        }
                    }
                }
            }
        }

        let summary = summarize(&diffs);
        let status = if summary.error > 0 {
            "drift_detected".to_string()
        } else {
            "clean".to_string()
        };

        Ok(DriftReport {
            schema_version: 1,
            kind: "sdkwork.database.drift-report".to_string(),
            checked_at: Utc::now().to_rfc3339(),
            module_id: descriptor.module_id.clone(),
            service_code: descriptor.service_code.clone(),
            engine: engine_name(engine),
            status,
            summary,
            pending_migrations,
            live_tables,
            expected_tables,
            diffs,
        })
    }
}

fn severity_for(code: &str, policy: &DriftPolicy) -> String {
    policy
        .severity_overrides
        .get(code)
        .cloned()
        .unwrap_or_else(|| default_severity(code))
}

fn default_severity(code: &str) -> String {
    match code {
        "extra_table" | "extra_column" | "missing_index" => "warn".to_string(),
        "extra_index" => "info".to_string(),
        _ => "error".to_string(),
    }
}

fn summarize(diffs: &[DriftDiff]) -> DriftSummary {
    DriftSummary {
        error: diffs.iter().filter(|d| d.severity == "error").count() as u32,
        warn: diffs.iter().filter(|d| d.severity == "warn").count() as u32,
        info: diffs.iter().filter(|d| d.severity == "info").count() as u32,
    }
}

/// Returns true if `name` is one of the well-known framework internal tables.
fn is_framework_table(name: &str) -> bool {
    name.starts_with("sqlite_") || FRAMEWORK_HISTORY_TABLES.contains(&name)
}

fn is_auto_index(name: &str) -> bool {
    name.starts_with("sqlite_autoindex") || name.ends_with("_pkey")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_framework_table() {
        assert!(is_framework_table("sqlite_sequence"));
        assert!(is_framework_table("ops_schema_migration_history"));
        assert!(is_framework_table("ops_seed_history"));
        assert!(is_framework_table("ops_database_installation_state"));
        assert!(!is_framework_table("ops_custom_table"));
        assert!(!is_framework_table("users"));
    }

    #[test]
    fn test_default_severity() {
        assert_eq!(default_severity("missing_table"), "error");
        assert_eq!(default_severity("extra_table"), "warn");
        assert_eq!(default_severity("extra_index"), "info");
        assert_eq!(default_severity("index_mismatch"), "error");
        assert_eq!(default_severity("sqlite_rowid_alias"), "error");
        assert_eq!(default_severity("checksum_mismatch"), "error");
    }
}
