//! Common types for database operations.
//!
//! This module provides common types that are used across the SDKWork database framework.
//! These types provide standardized patterns for common database patterns like timestamps,
//! soft deletion, versioning, and pagination.

use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

/// A timestamp that automatically updates on insert and update.
///
/// Use this for entities that need to track when they were created and last updated.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_repository::types::AutoTimestamp;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct User {
///     id: i64,
///     name: String,
///     #[serde(flatten)]
///     timestamp: AutoTimestamp,
/// }
///
/// let mut user = User {
///     id: 1,
///     name: "Alice".into(),
///     timestamp: AutoTimestamp::default(),
/// };
///
/// // Later, when updating:
/// user.timestamp.touch();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTimestamp {
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl Default for AutoTimestamp {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        Self {
            created_at: now,
            updated_at: now,
        }
    }
}

impl AutoTimestamp {
    /// Update the updated_at timestamp to the current time.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now().naive_utc();
    }

    /// Create a new timestamp with the given created_at time.
    pub fn with_created_at(created_at: NaiveDateTime) -> Self {
        Self {
            created_at,
            updated_at: created_at,
        }
    }
}

/// A soft-deletable entity.
///
/// Use this for entities that should be "deleted" but kept in the database
/// for audit purposes or data recovery.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_repository::types::SoftDelete;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct User {
///     id: i64,
///     name: String,
///     #[serde(flatten)]
///     soft_delete: SoftDelete,
/// }
///
/// let mut user = User {
///     id: 1,
///     name: "Alice".into(),
///     soft_delete: SoftDelete::default(),
/// };
///
/// // Soft delete
/// user.soft_delete.delete(Some("admin".into()));
/// assert!(user.soft_delete.is_deleted());
///
/// // Restore
/// user.soft_delete.restore();
/// assert!(!user.soft_delete.is_deleted());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SoftDelete {
    pub deleted_at: Option<NaiveDateTime>,
    pub deleted_by: Option<String>,
}

impl SoftDelete {
    /// Check if the entity is deleted.
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Mark the entity as deleted.
    pub fn delete(&mut self, deleted_by: Option<String>) {
        self.deleted_at = Some(Utc::now().naive_utc());
        self.deleted_by = deleted_by;
    }

    /// Restore the entity (undo soft delete).
    pub fn restore(&mut self) {
        self.deleted_at = None;
        self.deleted_by = None;
    }

    /// Get the deleted_at timestamp if deleted.
    pub fn deleted_at(&self) -> Option<NaiveDateTime> {
        self.deleted_at
    }

    /// Get who deleted the entity.
    pub fn deleted_by(&self) -> Option<&str> {
        self.deleted_by.as_deref()
    }
}

/// A versioned entity for optimistic locking.
///
/// Use this for entities that need optimistic concurrency control.
/// The version is automatically incremented on each update.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_repository::types::Versioned;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct User {
///     id: i64,
///     name: String,
///     #[serde(flatten)]
///     versioned: Versioned,
/// }
///
/// let mut user = User {
///     id: 1,
///     name: "Alice".into(),
///     versioned: Versioned::default(), // version = 1
/// };
///
/// // Update and increment version
/// user.name = "Alice Updated".into();
/// user.versioned.increment(); // version = 2
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Versioned {
    pub version: i64,
}

impl Default for Versioned {
    fn default() -> Self {
        Self { version: 1 }
    }
}

impl Versioned {
    /// Increment the version.
    pub fn increment(&mut self) {
        self.version += 1;
    }

    /// Get the current version.
    pub fn version(&self) -> i64 {
        self.version
    }

    /// Check if the version matches an expected value.
    pub fn matches(&self, expected: i64) -> bool {
        self.version == expected
    }
}

/// Common pagination parameters.
///
/// Use this for paginated queries. It provides methods to calculate
/// SQL LIMIT and OFFSET values.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_repository::types::Pagination;
///
/// let pagination = Pagination::new(2, 10); // Page 2, 10 items per page
/// assert_eq!(pagination.offset(), 10); // OFFSET 10
/// assert_eq!(pagination.limit(), 10);  // LIMIT 10
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub page: i64,
    pub per_page: i64,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

impl Pagination {
    /// Create new pagination parameters.
    ///
    /// - `page`: Page number (minimum 1)
    /// - `per_page`: Items per page (minimum 1, maximum 100)
    pub fn new(page: i64, per_page: i64) -> Self {
        Self {
            page: page.max(1),
            per_page: per_page.clamp(1, 100),
        }
    }

    /// Get the offset for SQL queries.
    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.per_page
    }

    /// Get the limit for SQL queries.
    pub fn limit(&self) -> i64 {
        self.per_page
    }

    /// Calculate the total number of pages.
    pub fn total_pages(&self, total_items: i64) -> i64 {
        (total_items as f64 / self.per_page as f64).ceil() as i64
    }

    /// Check if there is a next page.
    pub fn has_next(&self, total_items: i64) -> bool {
        self.page < self.total_pages(total_items)
    }

    /// Check if there is a previous page.
    pub fn has_previous(&self) -> bool {
        self.page > 1
    }
}

/// A paginated response.
///
/// Contains the data and pagination metadata for a paginated query.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_repository::types::{Pagination, PaginatedResponse};
///
/// let pagination = Pagination::new(1, 10);
/// let users = vec!["user1", "user2", "user3"];
/// let total = 100;
///
/// let response = PaginatedResponse::new(users, total, &pagination);
/// assert_eq!(response.data.len(), 3);
/// assert_eq!(response.total, 100);
/// assert_eq!(response.page, 1);
/// assert_eq!(response.per_page, 10);
/// assert_eq!(response.total_pages, 10);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

impl<T> PaginatedResponse<T> {
    /// Create a new paginated response.
    pub fn new(data: Vec<T>, total: i64, pagination: &Pagination) -> Self {
        Self {
            data,
            total,
            page: pagination.page,
            per_page: pagination.per_page,
            total_pages: pagination.total_pages(total),
        }
    }

    /// Check if there is a next page.
    pub fn has_next(&self) -> bool {
        self.page < self.total_pages
    }

    /// Check if there is a previous page.
    pub fn has_previous(&self) -> bool {
        self.page > 1
    }

    /// Get the number of items in this page.
    pub fn count(&self) -> usize {
        self.data.len()
    }
}

/// A filter for querying entities.
///
/// Provides a standardized way to specify filter criteria for queries.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_repository::types::QueryFilter;
/// use serde_json::Value;
///
/// let filter = QueryFilter::new()
///     .eq("status", Value::String("active".into()))
///     .gt("age", Value::Number(18.into()))
///     .like("name", "%alice%")
///     .order_by("created_at", false)
///     .limit(10);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryFilter {
    pub conditions: Vec<FilterCondition>,
    pub order_by: Vec<OrderBy>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOperator {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBy {
    pub column: String,
    pub ascending: bool,
}

impl QueryFilter {
    /// Create a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an equality condition.
    pub fn eq(mut self, column: &str, value: serde_json::Value) -> Self {
        self.conditions.push(FilterCondition {
            column: column.to_string(),
            operator: FilterOperator::Eq,
            value,
        });
        self
    }

    /// Add a not-equal condition.
    pub fn ne(mut self, column: &str, value: serde_json::Value) -> Self {
        self.conditions.push(FilterCondition {
            column: column.to_string(),
            operator: FilterOperator::Ne,
            value,
        });
        self
    }

    /// Add a greater-than condition.
    pub fn gt(mut self, column: &str, value: serde_json::Value) -> Self {
        self.conditions.push(FilterCondition {
            column: column.to_string(),
            operator: FilterOperator::Gt,
            value,
        });
        self
    }

    /// Add a greater-than-or-equal condition.
    pub fn gte(mut self, column: &str, value: serde_json::Value) -> Self {
        self.conditions.push(FilterCondition {
            column: column.to_string(),
            operator: FilterOperator::Gte,
            value,
        });
        self
    }

    /// Add a less-than condition.
    pub fn lt(mut self, column: &str, value: serde_json::Value) -> Self {
        self.conditions.push(FilterCondition {
            column: column.to_string(),
            operator: FilterOperator::Lt,
            value,
        });
        self
    }

    /// Add a less-than-or-equal condition.
    pub fn lte(mut self, column: &str, value: serde_json::Value) -> Self {
        self.conditions.push(FilterCondition {
            column: column.to_string(),
            operator: FilterOperator::Lte,
            value,
        });
        self
    }

    /// Add a LIKE condition.
    pub fn like(mut self, column: &str, pattern: &str) -> Self {
        self.conditions.push(FilterCondition {
            column: column.to_string(),
            operator: FilterOperator::Like,
            value: serde_json::Value::String(pattern.to_string()),
        });
        self
    }

    /// Add an IS NULL condition.
    pub fn is_null(mut self, column: &str) -> Self {
        self.conditions.push(FilterCondition {
            column: column.to_string(),
            operator: FilterOperator::IsNull,
            value: serde_json::Value::Null,
        });
        self
    }

    /// Add an IS NOT NULL condition.
    pub fn is_not_null(mut self, column: &str) -> Self {
        self.conditions.push(FilterCondition {
            column: column.to_string(),
            operator: FilterOperator::IsNotNull,
            value: serde_json::Value::Null,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_timestamp() {
        let ts = AutoTimestamp::default();
        assert_eq!(ts.created_at, ts.updated_at);

        let mut ts2 = ts.clone();
        ts2.touch();
        assert!(ts2.updated_at >= ts.updated_at);
    }

    #[test]
    fn test_soft_delete() {
        let mut sd = SoftDelete::default();
        assert!(!sd.is_deleted());

        sd.delete(Some("admin".into()));
        assert!(sd.is_deleted());
        assert_eq!(sd.deleted_by(), Some("admin"));

        sd.restore();
        assert!(!sd.is_deleted());
    }

    #[test]
    fn test_versioned() {
        let mut v = Versioned::default();
        assert_eq!(v.version(), 1);
        assert!(v.matches(1));

        v.increment();
        assert_eq!(v.version(), 2);
        assert!(!v.matches(1));
    }

    #[test]
    fn test_pagination() {
        let p = Pagination::new(2, 10);
        assert_eq!(p.offset(), 10);
        assert_eq!(p.limit(), 10);
        assert_eq!(p.total_pages(100), 10);
        assert!(p.has_next(100));
        assert!(p.has_previous());
    }

    #[test]
    fn test_paginated_response() {
        let p = Pagination::new(1, 10);
        let data = vec![1, 2, 3];
        let resp = PaginatedResponse::new(data, 100, &p);

        assert_eq!(resp.count(), 3);
        assert_eq!(resp.total, 100);
        assert_eq!(resp.total_pages, 10);
        assert!(resp.has_next());
        assert!(!resp.has_previous());
    }

    #[test]
    fn test_query_filter() {
        let filter = QueryFilter::new()
            .eq("status", serde_json::Value::String("active".into()))
            .gt("age", serde_json::Value::Number(18.into()))
            .order_by("created_at", false)
            .limit(10);

        assert_eq!(filter.conditions.len(), 2);
        assert_eq!(filter.order_by.len(), 1);
        assert_eq!(filter.limit, Some(10));
    }
}
