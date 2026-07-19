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

    let tokens = tokenize_sql(predicate);
    Some(canonicalize_predicate_tokens(&tokens).join(" "))
}

fn canonicalize_predicate_tokens(tokens: &[String]) -> Vec<String> {
    let tokens = strip_outer_parentheses(tokens);

    if let Some(parts) = split_top_level(tokens, "or") {
        return join_boolean_parts("or", &parts);
    }
    if let Some(parts) = split_top_level(tokens, "and") {
        return join_boolean_parts("and", &parts);
    }
    if tokens.first().map(String::as_str) == Some("not") && tokens.len() > 1 {
        return canonical_not(&tokens[1..]);
    }

    canonicalize_atomic_predicate(tokens)
}

fn join_boolean_parts(operator: &str, parts: &[&[String]]) -> Vec<String> {
    let mut canonical = vec!["(".to_string()];
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            canonical.push(operator.to_string());
        }
        canonical.extend(canonicalize_predicate_tokens(part));
    }
    canonical.push(")".to_string());
    canonical
}

fn canonicalize_atomic_predicate(tokens: &[String]) -> Vec<String> {
    if let Some(position) = find_top_level_token(tokens, "in") {
        let is_not_in = position > 0 && tokens[position - 1] == "not";
        if !is_not_in {
            if let Some(values) = parse_parenthesized_list(&tokens[position + 1..]) {
                return canonical_membership(&tokens[..position], &values);
            }
        }
    }

    if let Some(position) = find_top_level_token(tokens, "=") {
        let left = &tokens[..position];
        let right = &tokens[position + 1..];
        if let Some(values) = parse_any_array(right) {
            return canonical_membership(left, &values);
        }
        if let Some(values) = parse_any_array(left) {
            return canonical_membership(right, &values);
        }
        if let Some(value) = boolean_literal(right) {
            return canonical_boolean_comparison(left, value);
        }
        if let Some(value) = boolean_literal(left) {
            return canonical_boolean_comparison(right, value);
        }
    }

    if let Some(position) = find_top_level_token(tokens, "is") {
        let operand = &tokens[..position];
        let test = strip_outer_parentheses(&tokens[position + 1..]);
        match test {
            [value] if value == "true" => return canonicalize_predicate_tokens(operand),
            [value] if value == "false" => return canonical_not(operand),
            [not, value] if not == "not" && value == "false" => {
                return canonicalize_predicate_tokens(operand);
            }
            [not, value] if not == "not" && value == "true" => return canonical_not(operand),
            _ => {}
        }
    }

    canonicalize_scalar_tokens(tokens)
}

fn canonical_boolean_comparison(operand: &[String], value: bool) -> Vec<String> {
    if value {
        canonicalize_predicate_tokens(operand)
    } else {
        canonical_not(operand)
    }
}

fn canonical_not(tokens: &[String]) -> Vec<String> {
    let mut canonical = vec!["not".to_string(), "(".to_string()];
    canonical.extend(canonicalize_predicate_tokens(tokens));
    canonical.push(")".to_string());
    canonical
}

fn canonical_membership(left: &[String], values: &[&[String]]) -> Vec<String> {
    let mut canonical = canonicalize_scalar_tokens(left);
    canonical.extend(["in".to_string(), "(".to_string()]);
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            canonical.push(",".to_string());
        }
        canonical.extend(canonicalize_scalar_tokens(value));
    }
    canonical.push(")".to_string());
    canonical
}

fn canonicalize_scalar_tokens(tokens: &[String]) -> Vec<String> {
    strip_outer_parentheses(tokens).to_vec()
}

fn boolean_literal(tokens: &[String]) -> Option<bool> {
    match strip_outer_parentheses(tokens) {
        [value] if value == "true" => Some(true),
        [value] if value == "false" => Some(false),
        _ => None,
    }
}

fn parse_any_array(tokens: &[String]) -> Option<Vec<&[String]>> {
    let tokens = strip_outer_parentheses(tokens);
    if tokens.first().map(String::as_str) != Some("any") || tokens.len() < 4 {
        return None;
    }
    let arguments = &tokens[1..];
    if !outer_parentheses_wrap_all(arguments) {
        return None;
    }
    let array = strip_outer_parentheses(&arguments[1..arguments.len() - 1]);
    if array.first().map(String::as_str) != Some("array") {
        return None;
    }
    parse_bracketed_list(&array[1..])
}

fn parse_parenthesized_list(tokens: &[String]) -> Option<Vec<&[String]>> {
    if !outer_parentheses_wrap_all(tokens) {
        return None;
    }
    split_top_level_list(&tokens[1..tokens.len() - 1])
}

fn parse_bracketed_list(tokens: &[String]) -> Option<Vec<&[String]>> {
    if !outer_brackets_wrap_all(tokens) {
        return None;
    }
    split_top_level_list(&tokens[1..tokens.len() - 1])
}

fn split_top_level_list(tokens: &[String]) -> Option<Vec<&[String]>> {
    if tokens.is_empty() {
        return None;
    }
    let parts = split_at_top_level(tokens, ",");
    parts.iter().all(|part| !part.is_empty()).then_some(parts)
}

fn split_top_level<'a>(tokens: &'a [String], separator: &str) -> Option<Vec<&'a [String]>> {
    let parts = split_at_top_level(tokens, separator);
    (parts.len() > 1 && parts.iter().all(|part| !part.is_empty())).then_some(parts)
}

fn split_at_top_level<'a>(tokens: &'a [String], separator: &str) -> Vec<&'a [String]> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut parentheses = 0_i32;
    let mut brackets = 0_i32;

    for (position, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "(" => parentheses += 1,
            ")" => parentheses -= 1,
            "[" => brackets += 1,
            "]" => brackets -= 1,
            _ if token == separator && parentheses == 0 && brackets == 0 => {
                parts.push(&tokens[start..position]);
                start = position + 1;
            }
            _ => {}
        }
    }
    parts.push(&tokens[start..]);
    parts
}

fn find_top_level_token(tokens: &[String], target: &str) -> Option<usize> {
    let mut parentheses = 0_i32;
    let mut brackets = 0_i32;
    for (position, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "(" => parentheses += 1,
            ")" => parentheses -= 1,
            "[" => brackets += 1,
            "]" => brackets -= 1,
            _ if token == target && parentheses == 0 && brackets == 0 => return Some(position),
            _ => {}
        }
    }
    None
}

fn strip_outer_parentheses(mut tokens: &[String]) -> &[String] {
    while outer_parentheses_wrap_all(tokens) {
        tokens = &tokens[1..tokens.len() - 1];
    }
    tokens
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

        let array_bracket = current == '[' && tokens.last().map(String::as_str) == Some("array");
        if matches!(current, '\'' | '"' | '`') || (current == '[' && !array_bracket) {
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

fn outer_brackets_wrap_all(tokens: &[String]) -> bool {
    if tokens.len() < 2 || tokens.first().map(String::as_str) != Some("[") {
        return false;
    }
    if tokens.last().map(String::as_str) != Some("]") {
        return false;
    }

    let mut depth = 0_i32;
    for (position, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "[" => depth += 1,
            "]" => {
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

    #[test]
    fn predicate_normalization_matches_postgres_equivalent_forms() {
        assert!(predicates_match(
            Some("auto_renew = true AND status = 1"),
            Some("((auto_renew = true) AND (status = 1))")
        ));
        assert!(predicates_match(
            Some("status IN (0, 1, 2)"),
            Some("status = ANY (ARRAY[0, 1, 2])")
        ));
        assert!(predicates_match(
            Some("auto_renew = TRUE"),
            Some("auto_renew")
        ));
        assert!(predicates_match(
            Some("auto_renew = false"),
            Some("NOT auto_renew")
        ));
        assert!(predicates_match(
            Some("auto_renew IS NOT FALSE"),
            Some("auto_renew = true")
        ));
    }

    #[test]
    fn predicate_normalization_rejects_semantic_differences() {
        assert!(!predicates_match(
            Some("status IN (0, 1, 2)"),
            Some("status = ANY (ARRAY[0, 1, 3])")
        ));
        assert!(!predicates_match(
            Some("auto_renew = true"),
            Some("auto_renew = false")
        ));
        assert!(!predicates_match(
            Some("status = 1 AND auto_renew = true"),
            Some("status = 1 OR auto_renew = true")
        ));
        assert!(!predicates_match(
            Some("(status = 1 OR status = 2) AND auto_renew = true"),
            Some("status = 1 OR (status = 2 AND auto_renew = true)")
        ));
        assert!(!predicates_match(
            Some("state IN ('ACTIVE')"),
            Some("state = ANY (ARRAY['active'])")
        ));
    }
}
