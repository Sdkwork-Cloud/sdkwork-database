use chrono::Utc;
use sdkwork_database_contract::{
    load_expected_column_required, load_expected_column_types, load_expected_columns,
    load_expected_constraints, load_expected_indexes, load_expected_tables, physical_type_matches,
};
use sdkwork_database_history::{list_applied_migration_versions, migration_checksum};
use sdkwork_database_spi::types::DriftPolicy;
use sdkwork_database_sqlx::DatabasePool;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::DriftError;
use crate::introspect::{
    engine_name, introspect_table_column_details, introspect_table_constraints,
    introspect_table_indexes, introspect_tables,
};

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DriftSummary {
    pub error: u32,
    pub warn: u32,
    pub info: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftDiff {
    pub code: String,
    pub severity: String,
    pub message: String,
}

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

    pub async fn analyze(&self) -> Result<DriftReport, DriftError> {
        let descriptor = self.module.descriptor();
        let engine = self.pool.engine();
        let policy = self.module.load_policy().await?;
        let migrations = self.module.list_migrations(engine).await?;
        let applied =
            list_applied_migration_versions(&self.pool, &descriptor.module_id, engine).await?;

        let mut pending_migrations = Vec::new();
        let mut diffs = Vec::new();
        let known_versions = migrations
            .iter()
            .map(|migration| migration.version.clone())
            .collect::<Vec<_>>();

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

            let checksum = sdkwork_database_history::file_checksum(&migration.up_path)?;
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
            if !known_versions.iter().any(|known| known == version) {
                diffs.push(DriftDiff {
                    code: "migration_unknown".to_string(),
                    severity: severity_for("migration_unknown", &policy),
                    message: format!("migration_unknown: {version}"),
                });
            }
        }

        let live_tables = introspect_tables(&self.pool).await?;
        let contract_path = self.module.contract_path();
        let table_registry_path = contract_path
            .parent()
            .map(|dir| dir.join("table-registry.json"))
            .unwrap_or_default();
        let expected_tables = if contract_path.exists() {
            load_expected_tables(&contract_path, &table_registry_path)?
        } else {
            Vec::new()
        };

        for expected in &expected_tables {
            if policy.ignore_tables.iter().any(|item| item == expected) {
                continue;
            }
            if !live_tables.iter().any(|table| table == expected) {
                diffs.push(DriftDiff {
                    code: "missing_table".to_string(),
                    severity: severity_for("missing_table", &policy),
                    message: format!("missing table: {expected}"),
                });
            }
        }

        for live in &live_tables {
            if is_ignored_ops_table(live) || policy.ignore_tables.iter().any(|item| item == live) {
                continue;
            }
            if !expected_tables.is_empty() && !expected_tables.iter().any(|table| table == live) {
                diffs.push(DriftDiff {
                    code: "extra_table".to_string(),
                    severity: severity_for("extra_table", &policy),
                    message: format!("extra table: {live}"),
                });
            }
        }

        if contract_path.exists() {
            let expected_columns = load_expected_columns(&contract_path)?;
            let expected_column_types = load_expected_column_types(&contract_path)?;
            let expected_column_required = load_expected_column_required(&contract_path)?;
            let expected_indexes = load_expected_indexes(&contract_path)?;
            let expected_constraints = load_expected_constraints(&contract_path)?;
            if !expected_columns.is_empty() || !expected_column_types.is_empty() {
                let live_column_details = introspect_table_column_details(&self.pool).await?;
                for (table_name, expected) in expected_columns {
                    if policy.ignore_tables.iter().any(|item| item == &table_name) {
                        continue;
                    }
                    let live = live_column_details
                        .get(&table_name)
                        .cloned()
                        .unwrap_or_default();
                    let live_names = live
                        .iter()
                        .map(|column| column.name.clone())
                        .collect::<Vec<_>>();
                    for column in &expected {
                        if policy.ignore_columns.iter().any(|item| item == column) {
                            continue;
                        }
                        if !live_names.iter().any(|name| name == column) {
                            diffs.push(DriftDiff {
                                code: "missing_column".to_string(),
                                severity: severity_for("missing_column", &policy),
                                message: format!("missing column: {table_name}.{column}"),
                            });
                        }
                    }
                    for column in live {
                        if policy
                            .ignore_columns
                            .iter()
                            .any(|item| item == &column.name)
                        {
                            continue;
                        }
                        if !expected.iter().any(|name| name == &column.name) {
                            diffs.push(DriftDiff {
                                code: "extra_column".to_string(),
                                severity: severity_for("extra_column", &policy),
                                message: format!("extra column: {table_name}.{}", column.name),
                            });
                            continue;
                        }
                        if let Some(expected_types) = expected_column_types.get(&table_name) {
                            if let Some(logical_type) = expected_types.get(&column.name) {
                                if !physical_type_matches(logical_type, &column.data_type) {
                                    diffs.push(DriftDiff {
                                        code: "type_mismatch".to_string(),
                                        severity: severity_for("type_mismatch", &policy),
                                        message: format!(
                                            "type_mismatch: {table_name}.{} expected={logical_type} actual={}",
                                            column.name, column.data_type
                                        ),
                                    });
                                }
                            }
                        }
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
            if !expected_indexes.is_empty() || !expected_constraints.is_empty() {
                let live_indexes = introspect_table_indexes(&self.pool).await?;
                let expected_constraint_names: BTreeMap<String, Vec<String>> = expected_constraints
                    .iter()
                    .map(|(table, constraints)| {
                        (
                            table.clone(),
                            constraints.iter().map(|entry| entry.name.clone()).collect(),
                        )
                    })
                    .collect();

                for (table_name, indexes) in &expected_indexes {
                    if policy.ignore_tables.iter().any(|item| item == table_name) {
                        continue;
                    }
                    let live = live_indexes.get(table_name).cloned().unwrap_or_default();
                    for index in indexes {
                        if !live.iter().any(|name| name == &index.name) {
                            diffs.push(DriftDiff {
                                code: "missing_index".to_string(),
                                severity: severity_for("missing_index", &policy),
                                message: format!("missing index: {table_name}.{}", index.name),
                            });
                        }
                    }
                }

                for (table_name, live_index_names) in live_indexes {
                    if policy.ignore_tables.iter().any(|item| item == &table_name) {
                        continue;
                    }
                    let expected_idx_names = expected_indexes
                        .get(&table_name)
                        .map(|indexes| {
                            indexes
                                .iter()
                                .map(|index| index.name.clone())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let expected_cons_names = expected_constraint_names
                        .get(&table_name)
                        .cloned()
                        .unwrap_or_default();
                    for index_name in live_index_names {
                        if is_auto_index(&index_name) {
                            continue;
                        }
                        if expected_idx_names.iter().any(|name| name == &index_name) {
                            continue;
                        }
                        if expected_cons_names.iter().any(|name| name == &index_name) {
                            continue;
                        }
                        diffs.push(DriftDiff {
                            code: "extra_index".to_string(),
                            severity: severity_for("extra_index", &policy),
                            message: format!("extra index: {table_name}.{index_name}"),
                        });
                    }
                }
            }

            if !expected_constraints.is_empty() {
                let live_constraints = introspect_table_constraints(&self.pool).await?;
                for (table_name, constraints) in &expected_constraints {
                    if policy.ignore_tables.iter().any(|item| item == table_name) {
                        continue;
                    }
                    let live = live_constraints
                        .get(table_name)
                        .cloned()
                        .unwrap_or_default();
                    for constraint in constraints {
                        if !live.iter().any(|name| name == &constraint.name) {
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
        error: diffs.iter().filter(|diff| diff.severity == "error").count() as u32,
        warn: diffs.iter().filter(|diff| diff.severity == "warn").count() as u32,
        info: diffs.iter().filter(|diff| diff.severity == "info").count() as u32,
    }
}

fn is_ignored_ops_table(name: &str) -> bool {
    name.starts_with("sqlite_") || name.starts_with("ops_")
}

fn is_auto_index(name: &str) -> bool {
    name.starts_with("sqlite_autoindex") || name.ends_with("_pkey")
}
