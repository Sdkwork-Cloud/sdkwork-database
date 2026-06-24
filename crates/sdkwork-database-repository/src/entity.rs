use serde::{Deserialize, Serialize};

/// Trait for database entities.
///
/// This trait defines the basic interface that all database entities must implement.
/// It provides methods for getting the table name, primary key, and column definitions.
///
/// # Requirements
///
/// - Entity must be `Send + Sync + Clone + Serialize + Deserialize`
/// - Entity must have a primary key field of type `i64`
/// - All fields must be convertible to/from `serde_json::Value`
///
/// # Example
///
/// ```rust,ignore
/// use sdkwork_database_repository::prelude::*;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct User {
///     id: i64,
///     name: String,
///     email: String,
///     status: String,
/// }
///
/// impl_entity!(User, "users", id, [id, name, email, status]);
/// ```
pub trait Entity: Send + Sync + Clone + Serialize + for<'de> Deserialize<'de> {
    /// Get the table name for this entity.
    fn table_name() -> &'static str;

    /// Get the primary key value for this entity.
    fn primary_key(&self) -> i64;

    /// Get the column names for this entity.
    fn columns() -> &'static [&'static str];

    /// Get the column values for this entity as a JSON value.
    fn to_json(&self) -> serde_json::Value;

    /// Create an instance of this entity from a SQLite row.
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error>;

    /// Create an instance of this entity from a PostgreSQL row.
    fn from_pg_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error>;

    /// Get the number of columns.
    fn column_count() -> usize {
        Self::columns().len()
    }

    /// Check if a column exists.
    fn has_column(column: &str) -> bool {
        Self::columns().contains(&column)
    }

    /// Get the primary key column name.
    fn primary_key_column() -> &'static str {
        "id"
    }
}

/// Trait for entities that support auto ID generation via sdkwork-id.
///
/// Implement this trait to enable automatic ID generation on insert.
/// The Repository will call `generate_id()` before inserting if the ID is 0.
pub trait AutoIdEntity: Entity {
    /// Get a mutable reference to the primary key field.
    fn set_primary_key(&mut self, id: i64);

    /// Check if the primary key needs to be generated (is zero).
    fn needs_id(&self) -> bool {
        self.primary_key() == 0
    }
}

/// Trait for entities with String-based primary keys that support auto ID generation.
///
/// This is for entities where the primary key is a String (e.g., UUID or Snowflake as string).
pub trait StringIdEntity: Entity {
    /// Get the primary key as a string.
    fn primary_key_str(&self) -> &str;

    /// Set the primary key from a string.
    fn set_primary_key_str(&mut self, id: &str);

    /// Check if the primary key needs to be generated (is empty).
    fn needs_id(&self) -> bool {
        self.primary_key_str().is_empty()
    }
}

/// Macro to implement Entity for a struct.
///
/// This macro generates the Entity trait implementation for a struct.
/// It automatically handles serialization/deserialization and row mapping.
///
/// # Arguments
///
/// - `$struct_name`: The struct type to implement Entity for
/// - `$table`: The database table name
/// - `$primary_key`: The primary key field name
/// - `[$($field),*]`: List of all fields to include in column mapping
///
/// # Example
///
/// ```rust,ignore
/// use sdkwork_database_repository::prelude::*;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct User {
///     id: i64,
///     name: String,
///     email: String,
///     created_at: chrono::NaiveDateTime,
///     updated_at: chrono::NaiveDateTime,
/// }
///
/// impl_entity!(User, "users", id, [id, name, email, created_at, updated_at]);
/// ```
#[macro_export]
macro_rules! impl_entity {
    ($struct_name:ident, $table:expr, $primary_key:ident, [$($field:ident),*]) => {
        impl $crate::entity::Entity for $struct_name {
            fn table_name() -> &'static str {
                $table
            }

            fn primary_key(&self) -> i64 {
                self.$primary_key
            }

            fn columns() -> &'static [&'static str] {
                &[$(stringify!($field)),*]
            }

            fn to_json(&self) -> serde_json::Value {
                serde_json::to_value(self).unwrap_or_default()
            }

            fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
                use sqlx::Row;
                Ok(Self {
                    $(
                        $field: row.get(stringify!($field)),
                    )*
                })
            }

            fn from_pg_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
                use sqlx::Row;
                Ok(Self {
                    $(
                        $field: row.get(stringify!($field)),
                    )*
                })
            }

            fn primary_key_column() -> &'static str {
                stringify!($primary_key)
            }
        }
    };
}

/// Macro to implement Entity with custom primary key column name.
///
/// Use this when your primary key column is not "id".
///
/// # Example
///
/// ```rust,ignore
/// use sdkwork_database_repository::prelude::*;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct Order {
///     order_id: i64,
///     customer_id: i64,
///     total: f64,
/// }
///
/// impl_entity_with_pk!(Order, "orders", order_id, "order_id", [order_id, customer_id, total]);
/// ```
#[macro_export]
macro_rules! impl_entity_with_pk {
    ($struct_name:ident, $table:expr, $primary_key:ident, $pk_column:expr, [$($field:ident),*]) => {
        impl $crate::entity::Entity for $struct_name {
            fn table_name() -> &'static str {
                $table
            }

            fn primary_key(&self) -> i64 {
                self.$primary_key
            }

            fn columns() -> &'static [&'static str] {
                &[$(stringify!($field)),*]
            }

            fn to_json(&self) -> serde_json::Value {
                serde_json::to_value(self).unwrap_or_default()
            }

            fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
                use sqlx::Row;
                Ok(Self {
                    $(
                        $field: row.get(stringify!($field)),
                    )*
                })
            }

            fn from_pg_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
                use sqlx::Row;
                Ok(Self {
                    $(
                        $field: row.get(stringify!($field)),
                    )*
                })
            }

            fn primary_key_column() -> &'static str {
                $pk_column
            }
        }
    };
}

/// Macro to implement Entity with String primary key.
///
/// Use this when your primary key is a String/TEXT type instead of i64.
///
/// # Example
///
/// ```rust,ignore
/// use sdkwork_database_repository::prelude::*;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct DriveNode {
///     id: String,
///     tenant_id: String,
///     name: String,
/// }
///
/// impl_entity_string_pk!(DriveNode, "dr_drive_node", id, [id, tenant_id, name]);
/// ```
#[macro_export]
macro_rules! impl_entity_string_pk {
    ($struct_name:ident, $table:expr, $primary_key:ident, [$($field:ident),*]) => {
        impl $crate::entity::Entity for $struct_name {
            fn table_name() -> &'static str {
                $table
            }

            fn primary_key(&self) -> i64 {
                // For string primary keys, return 0 as a placeholder
                // The actual primary key is accessed via primary_key_str()
                0
            }

            fn columns() -> &'static [&'static str] {
                &[$(stringify!($field)),*]
            }

            fn to_json(&self) -> serde_json::Value {
                serde_json::to_value(self).unwrap_or_default()
            }

            fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
                use sqlx::Row;
                Ok(Self {
                    $(
                        $field: row.get(stringify!($field)),
                    )*
                })
            }

            fn from_pg_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
                use sqlx::Row;
                Ok(Self {
                    $(
                        $field: row.get(stringify!($field)),
                    )*
                })
            }

            fn primary_key_column() -> &'static str {
                stringify!($primary_key)
            }
        }

        impl $struct_name {
            /// Get the primary key as a string.
            pub fn primary_key_str(&self) -> &str {
                &self.$primary_key
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestEntity {
        id: i64,
        name: String,
    }

    impl_entity!(TestEntity, "test_entities", id, [id, name]);

    #[test]
    fn test_entity_traits() {
        assert_eq!(TestEntity::table_name(), "test_entities");
        assert_eq!(TestEntity::column_count(), 2);
        assert!(TestEntity::has_column("id"));
        assert!(TestEntity::has_column("name"));
        assert!(!TestEntity::has_column("nonexistent"));
        assert_eq!(TestEntity::primary_key_column(), "id");
    }

    #[test]
    fn test_entity_primary_key() {
        let entity = TestEntity {
            id: 42,
            name: "test".to_string(),
        };
        assert_eq!(entity.primary_key(), 42);
    }
}
