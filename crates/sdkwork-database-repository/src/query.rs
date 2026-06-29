use serde_json::Value;

/// Query builder for database queries.
///
/// This provides a unified interface for building queries that work
/// with both SQLite and PostgreSQL.
///
/// # Security
///
/// All column names are validated against a whitelist pattern to prevent SQL injection.
/// Column names must match the pattern: `[a-zA-Z_][a-zA-Z0-9_]*` (valid SQL identifier).
/// Invalid column names will result in an error during `to_sql()`.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_repository::prelude::*;
/// use serde_json::Value;
///
/// let query = Query::new()
///     .and_eq("status", Value::String("active".to_string()))
///     .gt("age", Value::Number(18.into()))
///     .order_by("name", true)
///     .limit(10)
///     .offset(0);
///
/// let (sql, params) = query.to_sql().expect("valid query");
/// // sql: "WHERE status = $1 AND age > $2 ORDER BY name ASC LIMIT 10 OFFSET 0"
/// // params: ["active", "18"]
/// ```
#[derive(Debug, Clone, Default)]
pub struct Query {
    conditions: Vec<Condition>,
    order_by: Vec<OrderBy>,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Validates that a column name is a safe SQL identifier.
/// Returns true if the column name matches the pattern: `[a-zA-Z_][a-zA-Z0-9_]*`
fn is_valid_column_name(column: &str) -> bool {
    if column.is_empty() {
        return false;
    }
    let first_char = column.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return false;
    }
    column
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Error type for query validation failures.
#[derive(Debug, Clone)]
pub struct QueryValidationError {
    pub column: String,
    pub reason: String,
}

impl std::fmt::Display for QueryValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid column name '{}': {}", self.column, self.reason)
    }
}

impl std::error::Error for QueryValidationError {}

#[derive(Debug, Clone)]
struct Condition {
    column: String,
    operator: Operator,
    value: Value,
}

#[derive(Debug, Clone)]
enum Operator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    In,
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone)]
struct OrderBy {
    column: String,
    ascending: bool,
}

impl Query {
    /// Create a new empty query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a query with a single equality condition.
    pub fn eq(column: &str, value: Value) -> Self {
        let mut query = Self::new();
        query.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::Eq,
            value,
        });
        query
    }

    /// Add an equality condition.
    pub fn and_eq(mut self, column: &str, value: Value) -> Self {
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::Eq,
            value,
        });
        self
    }

    /// Add a not-equal condition.
    pub fn ne(mut self, column: &str, value: Value) -> Self {
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::Ne,
            value,
        });
        self
    }

    /// Add a greater-than condition.
    pub fn gt(mut self, column: &str, value: Value) -> Self {
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::Gt,
            value,
        });
        self
    }

    /// Add a greater-than-or-equal condition.
    pub fn gte(mut self, column: &str, value: Value) -> Self {
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::Gte,
            value,
        });
        self
    }

    /// Add a less-than condition.
    pub fn lt(mut self, column: &str, value: Value) -> Self {
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::Lt,
            value,
        });
        self
    }

    /// Add a less-than-or-equal condition.
    pub fn lte(mut self, column: &str, value: Value) -> Self {
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::Lte,
            value,
        });
        self
    }

    /// Add a LIKE condition.
    pub fn like(mut self, column: &str, pattern: &str) -> Self {
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::Like,
            value: Value::String(pattern.to_string()),
        });
        self
    }

    /// Add an IN condition.
    pub fn in_list(mut self, column: &str, values: Vec<impl Into<Value>>) -> Self {
        let values: Vec<Value> = values.into_iter().map(|v| v.into()).collect();
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::In,
            value: Value::Array(values),
        });
        self
    }

    /// Add an IS NULL condition.
    pub fn is_null(mut self, column: &str) -> Self {
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::IsNull,
            value: Value::Null,
        });
        self
    }

    /// Add an IS NOT NULL condition.
    pub fn is_not_null(mut self, column: &str) -> Self {
        self.conditions.push(Condition {
            column: column.to_string(),
            operator: Operator::IsNotNull,
            value: Value::Null,
        });
        self
    }

    /// Add an ORDER BY clause.
    pub fn order_by(mut self, column: &str, ascending: bool) -> Self {
        self.order_by.push(OrderBy {
            column: column.to_string(),
            ascending,
        });
        self
    }

    /// Set the LIMIT.
    pub fn limit(mut self, limit: i64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the OFFSET.
    pub fn offset(mut self, offset: i64) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Convert the query to SQL WHERE clause and parameters.
    ///
    /// Returns Result<(where_clause, params)> where where_clause starts with "WHERE" if there are conditions,
    /// and params contains the parameter values.
    ///
    /// # Errors
    ///
    /// Returns `QueryValidationError` if any column name fails validation.
    /// Column names must be valid SQL identifiers matching `[a-zA-Z_][a-zA-Z0-9_]*`.
    pub fn to_sql(&self) -> Result<(String, Vec<String>), QueryValidationError> {
        // Validate all column names first to prevent SQL injection
        for condition in &self.conditions {
            if !is_valid_column_name(&condition.column) {
                return Err(QueryValidationError {
                    column: condition.column.clone(),
                    reason: "column name contains invalid characters or doesn't match SQL identifier pattern".to_string(),
                });
            }
        }
        for order in &self.order_by {
            if !is_valid_column_name(&order.column) {
                return Err(QueryValidationError {
                    column: order.column.clone(),
                    reason: "column name contains invalid characters or doesn't match SQL identifier pattern".to_string(),
                });
            }
        }

        let mut params = Vec::new();
        let mut conditions = Vec::new();

        for condition in &self.conditions {
            let param_idx = params.len() + 1;
            match &condition.operator {
                Operator::Eq => {
                    conditions.push(format!("{} = ${}", condition.column, param_idx));
                    params.push(value_to_string(&condition.value));
                }
                Operator::Ne => {
                    conditions.push(format!("{} != ${}", condition.column, param_idx));
                    params.push(value_to_string(&condition.value));
                }
                Operator::Gt => {
                    conditions.push(format!("{} > ${}", condition.column, param_idx));
                    params.push(value_to_string(&condition.value));
                }
                Operator::Gte => {
                    conditions.push(format!("{} >= ${}", condition.column, param_idx));
                    params.push(value_to_string(&condition.value));
                }
                Operator::Lt => {
                    conditions.push(format!("{} < ${}", condition.column, param_idx));
                    params.push(value_to_string(&condition.value));
                }
                Operator::Lte => {
                    conditions.push(format!("{} <= ${}", condition.column, param_idx));
                    params.push(value_to_string(&condition.value));
                }
                Operator::Like => {
                    conditions.push(format!("{} LIKE ${}", condition.column, param_idx));
                    params.push(value_to_string(&condition.value));
                }
                Operator::In => {
                    if let Value::Array(values) = &condition.value {
                        let placeholders: Vec<String> = values
                            .iter()
                            .enumerate()
                            .map(|(i, _)| format!("${}", param_idx + i))
                            .collect();
                        conditions.push(format!(
                            "{} IN ({})",
                            condition.column,
                            placeholders.join(", ")
                        ));
                        for val in values {
                            params.push(value_to_string(val));
                        }
                    }
                }
                Operator::IsNull => {
                    conditions.push(format!("{} IS NULL", condition.column));
                }
                Operator::IsNotNull => {
                    conditions.push(format!("{} IS NOT NULL", condition.column));
                }
            }
        }

        let mut parts = Vec::new();

        if !conditions.is_empty() {
            parts.push(format!("WHERE {}", conditions.join(" AND ")));
        }

        if !self.order_by.is_empty() {
            let order_clauses: Vec<String> = self
                .order_by
                .iter()
                .map(|o| {
                    if o.ascending {
                        format!("{} ASC", o.column)
                    } else {
                        format!("{} DESC", o.column)
                    }
                })
                .collect();
            parts.push(format!("ORDER BY {}", order_clauses.join(", ")));
        }

        if let Some(limit) = self.limit {
            parts.push(format!("LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            parts.push(format!("OFFSET {}", offset));
        }

        Ok((parts.join(" "), params))
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) => "[]".to_string(),
        Value::Object(_) => "{}".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query() {
        let query = Query::new();
        let (sql, params) = query.to_sql().unwrap();
        assert_eq!(sql, "");
        assert!(params.is_empty());
    }

    #[test]
    fn test_single_condition() {
        let query = Query::eq("status", Value::String("active".to_string()));
        let (sql, params) = query.to_sql().unwrap();
        assert_eq!(sql, "WHERE status = $1");
        assert_eq!(params, vec!["active"]);
    }

    #[test]
    fn test_multiple_conditions() {
        let query = Query::new()
            .and_eq("status", Value::String("active".to_string()))
            .gt("age", Value::Number(18.into()));
        let (sql, params) = query.to_sql().unwrap();
        assert_eq!(sql, "WHERE status = $1 AND age > $2");
        assert_eq!(params, vec!["active", "18"]);
    }

    #[test]
    fn test_order_by() {
        let query = Query::new()
            .order_by("name", true)
            .order_by("created_at", false);
        let (sql, params) = query.to_sql().unwrap();
        assert_eq!(sql, "ORDER BY name ASC, created_at DESC");
        assert!(params.is_empty());
    }

    #[test]
    fn test_limit_offset() {
        let query = Query::new().limit(10).offset(20);
        let (sql, params) = query.to_sql().unwrap();
        assert_eq!(sql, "LIMIT 10 OFFSET 20");
        assert!(params.is_empty());
    }

    #[test]
    fn test_complex_query() {
        let query = Query::new()
            .and_eq("status", Value::String("active".to_string()))
            .gt("age", Value::Number(18.into()))
            .like("name", "%alice%")
            .order_by("created_at", false)
            .limit(10)
            .offset(0);
        let (sql, params) = query.to_sql().unwrap();
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 0"));
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_sql_injection_prevention() {
        // Test that malicious column names are rejected
        let mut query = Query::new();
        query.conditions.push(Condition {
            column: "id; DROP TABLE users; --".to_string(),
            operator: Operator::Eq,
            value: Value::String("malicious".to_string()),
        });

        let result = query.to_sql();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.column.contains("DROP TABLE"));
    }

    #[test]
    fn test_valid_column_names() {
        assert!(is_valid_column_name("status"));
        assert!(is_valid_column_name("_private"));
        assert!(is_valid_column_name("user_name"));
        assert!(is_valid_column_name("createdAt"));
        assert!(is_valid_column_name("CamelCase123"));
    }

    #[test]
    fn test_invalid_column_names() {
        assert!(!is_valid_column_name(""));
        assert!(!is_valid_column_name("123abc"));
        assert!(!is_valid_column_name("column-name"));
        assert!(!is_valid_column_name("column name"));
        assert!(!is_valid_column_name("column;DROP"));
        assert!(!is_valid_column_name("column'name"));
    }
}
