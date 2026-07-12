use sdkwork_database_config::DatabaseEngine;
use sdkwork_database_contract::{physical_type_matches, ConstraintContract, IndexContract};

use crate::introspect::{ConstraintInfo, IndexInfo};

pub(crate) fn physical_type_matches_for_engine(
    engine: DatabaseEngine,
    logical_type: &str,
    physical_type: &str,
) -> bool {
    if physical_type_matches(logical_type, physical_type) {
        return true;
    }

    if engine != DatabaseEngine::Sqlite {
        return false;
    }

    let logical_type = normalize_type(logical_type);
    let affinity = sqlite_affinity(physical_type);
    match logical_type.as_str() {
        "json" | "jsonb" | "decimal" | "numeric" | "uuid" => affinity == SqliteAffinity::Text,
        "bool" | "boolean" => affinity == SqliteAffinity::Integer,
        _ => false,
    }
}

pub(crate) fn constraint_is_satisfied(
    engine: DatabaseEngine,
    expected: &ConstraintContract,
    live_constraints: &[ConstraintInfo],
    live_indexes: &[IndexInfo],
) -> bool {
    let expected_type = normalize_type(&expected.constraint_type);
    match expected_type.as_str() {
        "primary_key" => live_constraints.iter().any(|live| {
            normalize_type(&live.constraint_type) == expected_type
                && columns_match(&expected.columns, &live.columns)
        }),
        "unique" => {
            unique_constraint_is_satisfied(engine, expected, live_constraints, live_indexes)
        }
        "foreign_key" => live_constraints.iter().any(|live| {
            normalize_type(&live.constraint_type) == expected_type
                && (live.name.as_deref() == Some(expected.name.as_str())
                    || foreign_key_structure_matches(expected, live))
        }),
        // SQLite does not expose CHECK constraints through PRAGMA metadata.
        // Treat them as unverifiable instead of reporting every valid CHECK as missing.
        "check" if engine == DatabaseEngine::Sqlite => true,
        _ => live_constraints.iter().any(|live| {
            normalize_type(&live.constraint_type) == expected_type
                && live.name.as_deref() == Some(expected.name.as_str())
        }),
    }
}

pub(crate) fn index_is_satisfied(expected: &IndexContract, live: &IndexInfo) -> bool {
    expected.name == live.name
        && expected.columns == live.columns
        && expected.unique == live.unique
        && predicates_match(expected.predicate.as_deref(), live.predicate.as_deref())
}

fn predicates_match(expected: Option<&str>, live: Option<&str>) -> bool {
    normalize_predicate(expected) == normalize_predicate(live)
}

fn normalize_predicate(predicate: Option<&str>) -> Option<String> {
    let predicate = predicate?.trim();
    if predicate.is_empty() {
        return None;
    }

    let mut tokens = tokenize_sql(predicate);
    while outer_parentheses_wrap_all(&tokens) {
        tokens.remove(0);
        tokens.pop();
    }
    Some(tokens.join(" "))
}

fn tokenize_sql(sql: &str) -> Vec<String> {
    let chars = sql.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let current = chars[index];
        if current.is_whitespace() {
            index += 1;
            continue;
        }

        if matches!(current, '\'' | '"' | '`' | '[') {
            let terminator = if current == '[' { ']' } else { current };
            let start = index;
            index += 1;
            while index < chars.len() {
                if chars[index] == terminator {
                    if terminator != ']'
                        && chars.get(index + 1).is_some_and(|next| *next == terminator)
                    {
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                index += 1;
            }
            tokens.push(chars[start..index].iter().collect());
            continue;
        }

        if current.is_alphanumeric() || matches!(current, '_' | '$') {
            let start = index;
            index += 1;
            while chars
                .get(index)
                .is_some_and(|value| value.is_alphanumeric() || matches!(value, '_' | '$'))
            {
                index += 1;
            }
            tokens.push(
                chars[start..index]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase(),
            );
            continue;
        }

        let mut operator = current.to_string();
        if let Some(next) = chars.get(index + 1) {
            let pair = format!("{current}{next}");
            if matches!(
                pair.as_str(),
                "<=" | ">=" | "<>" | "!=" | "==" | "||" | "&&" | "::" | "->" | "=>"
            ) {
                operator = pair;
                index += 1;
            }
        }
        tokens.push(operator);
        index += 1;
    }

    tokens
}

fn outer_parentheses_wrap_all(tokens: &[String]) -> bool {
    if tokens.len() < 2 || tokens.first().map(String::as_str) != Some("(") {
        return false;
    }
    if tokens.last().map(String::as_str) != Some(")") {
        return false;
    }

    let mut depth = 0_i32;
    for (position, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "(" => depth += 1,
            ")" => {
                depth -= 1;
                if depth == 0 && position + 1 != tokens.len() {
                    return false;
                }
            }
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

fn unique_constraint_is_satisfied(
    engine: DatabaseEngine,
    expected: &ConstraintContract,
    live_constraints: &[ConstraintInfo],
    live_indexes: &[IndexInfo],
) -> bool {
    if live_constraints.iter().any(|live| {
        normalize_type(&live.constraint_type) == "unique"
            && live.name.as_deref() == Some(expected.name.as_str())
    }) {
        return true;
    }

    live_indexes.iter().any(|index| {
        if !index.unique {
            return false;
        }
        if index.name == expected.name {
            return true;
        }
        engine == DatabaseEngine::Sqlite
            && index.name.starts_with("sqlite_autoindex")
            && columns_match(&expected.columns, &index.columns)
    })
}

fn foreign_key_structure_matches(expected: &ConstraintContract, live: &ConstraintInfo) -> bool {
    columns_match(&expected.columns, &live.columns)
        && expected
            .references_table
            .as_ref()
            .map_or(true, |table| live.references_table.as_ref() == Some(table))
        && (expected.references_columns.is_empty()
            || expected.references_columns == live.references_columns)
}

fn columns_match(expected: &[String], live: &[String]) -> bool {
    !expected.is_empty() && expected == live
}

fn normalize_type(value: &str) -> String {
    value
        .split('(')
        .next()
        .unwrap_or(value)
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "_")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SqliteAffinity {
    Integer,
    Text,
    Blob,
    Real,
    Numeric,
}

fn sqlite_affinity(physical_type: &str) -> SqliteAffinity {
    let physical_type = physical_type.trim().to_ascii_uppercase();
    if physical_type.contains("INT") {
        SqliteAffinity::Integer
    } else if physical_type.contains("CHAR")
        || physical_type.contains("CLOB")
        || physical_type.contains("TEXT")
    {
        SqliteAffinity::Text
    } else if physical_type.is_empty() || physical_type.contains("BLOB") {
        SqliteAffinity::Blob
    } else if physical_type.contains("REAL")
        || physical_type.contains("FLOA")
        || physical_type.contains("DOUB")
    {
        SqliteAffinity::Real
    } else {
        SqliteAffinity::Numeric
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constraint(name: &str, constraint_type: &str, columns: &[&str]) -> ConstraintContract {
        ConstraintContract {
            name: name.to_string(),
            constraint_type: constraint_type.to_string(),
            columns: columns.iter().map(|column| (*column).to_string()).collect(),
            references_table: None,
            references_columns: Vec::new(),
        }
    }

    #[test]
    fn sqlite_type_matching_accepts_canonical_storage_affinities() {
        assert!(physical_type_matches_for_engine(
            DatabaseEngine::Sqlite,
            "json",
            "TEXT"
        ));
        assert!(physical_type_matches_for_engine(
            DatabaseEngine::Sqlite,
            "decimal(38, 12)",
            "TEXT"
        ));
        assert!(physical_type_matches_for_engine(
            DatabaseEngine::Sqlite,
            "bool",
            "INTEGER"
        ));
        assert!(!physical_type_matches_for_engine(
            DatabaseEngine::Postgres,
            "json",
            "text"
        ));
    }

    #[test]
    fn postgres_unique_contract_requires_a_unique_index() {
        let expected = constraint("uk_probe_code", "unique", &["code"]);
        let non_unique = IndexInfo {
            name: expected.name.clone(),
            unique: false,
            columns: Vec::new(),
            predicate: None,
        };
        let unique = IndexInfo {
            unique: true,
            ..non_unique.clone()
        };

        assert!(!constraint_is_satisfied(
            DatabaseEngine::Postgres,
            &expected,
            &[],
            &[non_unique]
        ));
        assert!(constraint_is_satisfied(
            DatabaseEngine::Postgres,
            &expected,
            &[],
            &[unique]
        ));
    }

    #[test]
    fn primary_keys_match_by_ordered_columns_when_database_renames_them() {
        let expected = constraint("pk_probe", "primary_key", &["tenant_id", "id"]);
        let live = ConstraintInfo {
            name: Some("probe_pkey".to_string()),
            constraint_type: "primary_key".to_string(),
            columns: vec!["tenant_id".to_string(), "id".to_string()],
            references_table: None,
            references_columns: Vec::new(),
        };

        assert!(constraint_is_satisfied(
            DatabaseEngine::Postgres,
            &expected,
            &[live],
            &[]
        ));
    }

    #[test]
    fn sqlite_autoindexes_match_unique_constraints_by_columns() {
        let expected = constraint("uk_probe_code", "unique", &["code"]);
        let live = IndexInfo {
            name: "sqlite_autoindex_probe_1".to_string(),
            unique: true,
            columns: vec!["code".to_string()],
            predicate: None,
        };

        assert!(constraint_is_satisfied(
            DatabaseEngine::Sqlite,
            &expected,
            &[],
            &[live]
        ));
    }

    #[test]
    fn sqlite_foreign_keys_require_the_expected_reference() {
        let mut expected = constraint("fk_probe_parent", "foreign_key", &["parent_id"]);
        expected.references_table = Some("parent".to_string());
        expected.references_columns = vec!["id".to_string()];
        let live = ConstraintInfo {
            name: None,
            constraint_type: "foreign_key".to_string(),
            columns: vec!["parent_id".to_string()],
            references_table: Some("wrong_parent".to_string()),
            references_columns: vec!["id".to_string()],
        };

        assert!(!constraint_is_satisfied(
            DatabaseEngine::Sqlite,
            &expected,
            &[live],
            &[]
        ));
    }

    fn index(columns: &[&str], unique: bool, predicate: Option<&str>) -> IndexContract {
        IndexContract {
            name: "idx_probe".to_string(),
            columns: columns.iter().map(|column| (*column).to_string()).collect(),
            unique,
            predicate: predicate.map(str::to_string),
        }
    }

    #[test]
    fn indexes_require_ordered_columns_uniqueness_and_predicate() {
        let expected = index(&["tenant_id", "id"], true, Some("deleted_at IS NULL"));
        let matching = IndexInfo {
            name: expected.name.clone(),
            columns: expected.columns.clone(),
            unique: true,
            predicate: Some(" ( DELETED_AT   is null ) ".to_string()),
        };
        assert!(index_is_satisfied(&expected, &matching));

        let wrong_order = IndexInfo {
            columns: vec!["id".to_string(), "tenant_id".to_string()],
            ..matching.clone()
        };
        assert!(!index_is_satisfied(&expected, &wrong_order));

        let wrong_uniqueness = IndexInfo {
            unique: false,
            ..matching.clone()
        };
        assert!(!index_is_satisfied(&expected, &wrong_uniqueness));

        let wrong_predicate = IndexInfo {
            predicate: Some("deleted_at IS NOT NULL".to_string()),
            ..matching
        };
        assert!(!index_is_satisfied(&expected, &wrong_predicate));
    }

    #[test]
    fn predicate_normalization_preserves_string_literal_case() {
        assert!(predicates_match(
            Some("status = 'ACTIVE'"),
            Some("STATUS='ACTIVE'")
        ));
        assert!(!predicates_match(
            Some("status = 'ACTIVE'"),
            Some("status = 'active'")
        ));
    }
}
