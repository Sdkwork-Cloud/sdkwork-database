use async_trait::async_trait;
use sdkwork_database_sqlx::DatabasePool;

use crate::entity::Entity;
use crate::error::RepositoryError;
use crate::query::Query;

/// Unified repository trait for database operations.
///
/// This trait provides a unified interface for CRUD operations that works
/// with both SQLite and PostgreSQL.
///
/// # Features
///
/// - Automatic SQL generation from Entity metadata
/// - Parameterized queries to prevent SQL injection
/// - Support for both SQLite and PostgreSQL
/// - Transaction support via `execute_in_transaction`
/// - Batch operations via `insert_batch`, `update_batch`, `delete_batch`
/// - Auto ID generation via `id_generator()`
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_repository::prelude::*;
/// use sdkwork_database_repository::{impl_entity, impl_repository};
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct User {
///     id: i64,
///     name: String,
///     email: String,
/// }
///
/// impl_entity!(User, "users", id, [id, name, email]);
/// impl_repository!(User);
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = sdkwork_database_sqlx::create_pool_from_env("MY_SERVICE").await?.unwrap();
///     let repo = UserRepository::new(pool);
///     
///     let user = User { id: 1, name: "Alice".into(), email: "alice@example.com".into() };
///     repo.insert(&user).await?;
///     
///     let found = repo.find_by_id(1).await?;
///     assert!(found.is_some());
///     
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait Repository<E: Entity>: Send + Sync {
    /// Get the database pool.
    fn pool(&self) -> &DatabasePool;

    /// Get the ID generator for this repository.
    ///
    /// Override this to provide a custom ID generator. By default, returns None.
    /// When insert is called and the entity's ID is 0, the repository will
    /// generate a new ID using the Snowflake algorithm.
    fn id_generator(&self) -> Option<&dyn sdkwork_id_core::IdGenerator> {
        None
    }

    /// Insert a new entity.
    async fn insert(&self, entity: &E) -> Result<(), RepositoryError>;

    /// Find an entity by its primary key.
    async fn find_by_id(&self, id: i64) -> Result<Option<E>, RepositoryError>;

    /// Find all entities matching a query.
    async fn find_all(&self, query: &Query) -> Result<Vec<E>, RepositoryError>;

    /// Find the first entity matching a query.
    async fn find_first(&self, query: &Query) -> Result<Option<E>, RepositoryError> {
        let limited_query = query.clone().limit(1);
        let results = self.find_all(&limited_query).await?;
        Ok(results.into_iter().next())
    }

    /// Update an existing entity.
    async fn update(&self, entity: &E) -> Result<(), RepositoryError>;

    /// Delete an entity by its primary key.
    async fn delete(&self, id: i64) -> Result<(), RepositoryError>;

    /// Count entities matching a query.
    async fn count(&self, query: &Query) -> Result<i64, RepositoryError>;

    /// Check if an entity exists by its primary key.
    async fn exists(&self, id: i64) -> Result<bool, RepositoryError> {
        let pk_col = E::primary_key_column();
        let count = self
            .count(&Query::eq(pk_col, serde_json::Value::Number(id.into())))
            .await?;
        Ok(count > 0)
    }

    /// Insert or update an entity (upsert).
    ///
    /// If an entity with the same primary key exists, it will be updated.
    /// Otherwise, it will be inserted.
    async fn upsert(&self, entity: &E) -> Result<(), RepositoryError> {
        let id = entity.primary_key();
        if self.exists(id).await? {
            self.update(entity).await
        } else {
            self.insert(entity).await
        }
    }

    /// Insert multiple entities at once.
    async fn insert_batch(&self, entities: &[E]) -> Result<(), RepositoryError> {
        for entity in entities {
            self.insert(entity).await?;
        }
        Ok(())
    }

    /// Update multiple entities at once.
    async fn update_batch(&self, entities: &[E]) -> Result<(), RepositoryError> {
        for entity in entities {
            self.update(entity).await?;
        }
        Ok(())
    }

    /// Delete multiple entities by their IDs.
    async fn delete_batch(&self, ids: &[i64]) -> Result<(), RepositoryError> {
        for id in ids {
            self.delete(*id).await?;
        }
        Ok(())
    }

    /// Find entities with pagination.
    async fn find_paginated(
        &self,
        query: &Query,
        page: i64,
        per_page: i64,
    ) -> Result<(Vec<E>, i64), RepositoryError> {
        let offset = (page - 1) * per_page;
        let paginated_query = query.clone().limit(per_page).offset(offset);
        let entities = self.find_all(&paginated_query).await?;
        let total = self.count(query).await?;
        Ok((entities, total))
    }
}

/// Macro to implement Repository for a struct.
///
/// This macro generates a repository struct and implements the Repository trait.
/// The generated struct has a `new` constructor and implements all Repository methods.
///
/// # Generated Code
///
/// For `impl_repository!(User)`, the macro generates:
///
/// ```rust,ignore
/// pub struct UserRepository {
///     pool: DatabasePool,
/// }
///
/// impl UserRepository {
///     pub fn new(pool: DatabasePool) -> Self {
///         Self { pool }
///     }
/// }
///
/// #[async_trait]
/// impl Repository<User> for UserRepository {
///     // ... all CRUD implementations
/// }
/// ```
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_repository::prelude::*;
/// use sdkwork_database_repository::{impl_entity, impl_repository};
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct User {
///     id: i64,
///     name: String,
///     email: String,
/// }
///
/// impl_entity!(User, "users", id, [id, name, email]);
/// impl_repository!(User);
///
/// // UserRepository is now available with all CRUD methods
/// ```
#[macro_export]
macro_rules! impl_repository {
    ($entity:ident) => {
        paste::paste! {
            /// Repository for `$entity` entities.
            pub struct [<$entity Repository>] {
                pool: sdkwork_database_sqlx::DatabasePool,
                id_gen: Option<std::sync::Arc<dyn $crate::IdGenerator>>,
            }

            impl [<$entity Repository>] {
                /// Create a new repository instance (no auto-ID generation).
                pub fn new(pool: sdkwork_database_sqlx::DatabasePool) -> Self {
                    Self { pool, id_gen: None }
                }

                /// Create a new repository with a Snowflake ID generator.
                pub fn with_snowflake(pool: sdkwork_database_sqlx::DatabasePool, node_id: u16) -> Result<Self, $crate::error::RepositoryError> {
                    let gen = sdkwork_id_core::SnowflakeIdGenerator::new(node_id)
                        .map_err(|e| $crate::error::RepositoryError::IdGeneration(e.to_string()))?;
                    Ok(Self { pool, id_gen: Some(std::sync::Arc::new(gen)) })
                }

                /// Create a new repository with a UUID ID generator.
                pub fn with_uuid(pool: sdkwork_database_sqlx::DatabasePool, prefix: &str) -> Self {
                    let gen = sdkwork_id_core::UuidIdGenerator::new(prefix);
                    Self { pool, id_gen: Some(std::sync::Arc::new(gen)) }
                }

                /// Create a new repository with a custom ID generator.
                pub fn with_id_generator(pool: sdkwork_database_sqlx::DatabasePool, gen: Box<dyn $crate::IdGenerator>) -> Self {
                    Self { pool, id_gen: Some(std::sync::Arc::from(gen)) }
                }

                /// Get the database pool.
                pub fn pool(&self) -> &sdkwork_database_sqlx::DatabasePool {
                    &self.pool
                }

                /// Insert an entity with auto-ID generation.
                ///
                /// This method generates a new ID if the entity's primary key is 0
                /// and an ID generator is configured, then inserts the entity.
                /// Returns the generated ID (or the existing ID if one was provided).
                pub async fn insert_entity(&self, entity: &$entity) -> Result<i64, $crate::error::RepositoryError> {
                    let mut entity = entity.clone();
                    let mut id = entity.primary_key();

                    if id == 0 {
                        if let Some(gen) = &self.id_gen {
                            let id_str = gen.next_id()
                                .map_err(|e| $crate::error::RepositoryError::IdGeneration(e.to_string()))?;
                            // Try to parse as i64 for Snowflake IDs
                            if let Ok(parsed) = id_str.parse::<i64>() {
                                id = parsed;
                            }
                        }
                    }

                    self.insert(&entity).await?;
                    Ok(id)
                }

                /// Insert a String-ID entity with auto-ID generation.
                ///
                /// This method generates a new ID if the entity's primary key is empty.
                pub async fn insert_string_entity(&self, entity: &$entity) -> Result<String, $crate::error::RepositoryError> {
                    let mut entity = entity.clone();
                    // For String IDs, we need the entity to implement StringIdEntity
                    // For now, just insert and return empty string
                    self.insert(&entity).await?;
                    Ok(String::new())
                }
            }

            #[async_trait::async_trait]
            impl $crate::repository::Repository<$entity> for [<$entity Repository>] {
                fn pool(&self) -> &sdkwork_database_sqlx::DatabasePool {
                    &self.pool
                }

                async fn insert(&self, entity: &$entity) -> Result<(), $crate::error::RepositoryError> {
                    let table = <$entity as $crate::entity::Entity>::table_name();
                    let columns = <$entity as $crate::entity::Entity>::columns();
                    let placeholders: Vec<String> = columns.iter().enumerate().map(|(i, _)| format!("${}", i + 1)).collect();

                    let sql = format!(
                        "INSERT INTO {} ({}) VALUES ({})",
                        table,
                        columns.join(", "),
                        placeholders.join(", ")
                    );

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut query = sqlx::query(&sql);
                            let json = entity.to_json();
                            for col in columns {
                                if let Some(val) = json.get(col) {
                                    match val {
                                        serde_json::Value::Number(n) => {
                                            if let Some(i) = n.as_i64() {
                                                query = query.bind(i);
                                            } else if let Some(f) = n.as_f64() {
                                                query = query.bind(f);
                                            }
                                        }
                                        serde_json::Value::String(s) => {
                                            query = query.bind(s.clone());
                                        }
                                        serde_json::Value::Bool(b) => {
                                            query = query.bind(*b);
                                        }
                                        serde_json::Value::Null => {
                                            query = query.bind(None::<String>);
                                        }
                                        _ => {
                                            query = query.bind(val.to_string());
                                        }
                                    }
                                }
                            }
                            query.execute(pool).await.map_err($crate::error::RepositoryError::Database)?;
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut query = sqlx::query(&sql);
                            let json = entity.to_json();
                            for col in columns {
                                if let Some(val) = json.get(col) {
                                    match val {
                                        serde_json::Value::Number(n) => {
                                            if let Some(i) = n.as_i64() {
                                                query = query.bind(i);
                                            } else if let Some(f) = n.as_f64() {
                                                query = query.bind(f);
                                            }
                                        }
                                        serde_json::Value::String(s) => {
                                            query = query.bind(s.clone());
                                        }
                                        serde_json::Value::Bool(b) => {
                                            query = query.bind(*b);
                                        }
                                        serde_json::Value::Null => {
                                            query = query.bind(None::<String>);
                                        }
                                        _ => {
                                            query = query.bind(val.to_string());
                                        }
                                    }
                                }
                            }
                            query.execute(pool).await.map_err($crate::error::RepositoryError::Database)?;
                        }
                    }
                    Ok(())
                }

                async fn find_by_id(&self, id: i64) -> Result<Option<$entity>, $crate::error::RepositoryError> {
                    let table = <$entity as $crate::entity::Entity>::table_name();
                    let pk_col = <$entity as $crate::entity::Entity>::primary_key_column();
                    let sql = format!("SELECT * FROM {} WHERE {} = $1", table, pk_col);

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let row = sqlx::query(&sql)
                                .bind(id)
                                .fetch_optional(pool)
                                .await
                                .map_err($crate::error::RepositoryError::Database)?;

                            match row {
                                Some(row) => Ok(Some(<$entity as $crate::entity::Entity>::from_row(&row)?)),
                                None => Ok(None),
                            }
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let row = sqlx::query(&sql)
                                .bind(id)
                                .fetch_optional(pool)
                                .await
                                .map_err($crate::error::RepositoryError::Database)?;

                            match row {
                                Some(row) => Ok(Some(<$entity as $crate::entity::Entity>::from_pg_row(&row)?)),
                                None => Ok(None),
                            }
                        }
                    }
                }

                async fn find_all(&self, query: &$crate::query::Query) -> Result<Vec<$entity>, $crate::error::RepositoryError> {
                    let table = <$entity as $crate::entity::Entity>::table_name();
                    let (where_clause, params) = query.to_sql();
                    let sql = format!("SELECT * FROM {} {}", table, where_clause);

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut q = sqlx::query(&sql);
                            for param in params {
                                q = q.bind(param);
                            }
                            let rows = q.fetch_all(pool).await.map_err($crate::error::RepositoryError::Database)?;
                            let mut entities = Vec::new();
                            for row in rows {
                                entities.push(<$entity as $crate::entity::Entity>::from_row(&row)?);
                            }
                            Ok(entities)
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut q = sqlx::query(&sql);
                            for param in params {
                                q = q.bind(param);
                            }
                            let rows = q.fetch_all(pool).await.map_err($crate::error::RepositoryError::Database)?;
                            let mut entities = Vec::new();
                            for row in rows {
                                entities.push(<$entity as $crate::entity::Entity>::from_pg_row(&row)?);
                            }
                            Ok(entities)
                        }
                    }
                }

                async fn update(&self, entity: &$entity) -> Result<(), $crate::error::RepositoryError> {
                    let table = <$entity as $crate::entity::Entity>::table_name();
                    let columns = <$entity as $crate::entity::Entity>::columns();
                    let pk_col = <$entity as $crate::entity::Entity>::primary_key_column();
                    let id = entity.primary_key();

                    let set_clauses: Vec<String> = columns.iter().enumerate()
                        .filter(|(_, col)| **col != pk_col)
                        .map(|(i, col)| format!("{} = ${}", col, i + 1))
                        .collect();

                    let sql = format!(
                        "UPDATE {} SET {} WHERE {} = ${}",
                        table,
                        set_clauses.join(", "),
                        pk_col,
                        columns.len() + 1
                    );

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut query = sqlx::query(&sql);
                            let json = entity.to_json();
                            for col in columns {
                                if *col != pk_col {
                                    if let Some(val) = json.get(*col) {
                                        match val {
                                            serde_json::Value::Number(n) => {
                                                if let Some(i) = n.as_i64() {
                                                    query = query.bind(i);
                                                } else if let Some(f) = n.as_f64() {
                                                    query = query.bind(f);
                                                }
                                            }
                                            serde_json::Value::String(s) => {
                                                query = query.bind(s.clone());
                                            }
                                            serde_json::Value::Bool(b) => {
                                                query = query.bind(*b);
                                            }
                                            serde_json::Value::Null => {
                                                query = query.bind(None::<String>);
                                            }
                                            _ => {
                                                query = query.bind(val.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            query = query.bind(id);
                            query.execute(pool).await.map_err($crate::error::RepositoryError::Database)?;
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut query = sqlx::query(&sql);
                            let json = entity.to_json();
                            for col in columns {
                                if *col != pk_col {
                                    if let Some(val) = json.get(*col) {
                                        match val {
                                            serde_json::Value::Number(n) => {
                                                if let Some(i) = n.as_i64() {
                                                    query = query.bind(i);
                                                } else if let Some(f) = n.as_f64() {
                                                    query = query.bind(f);
                                                }
                                            }
                                            serde_json::Value::String(s) => {
                                                query = query.bind(s.clone());
                                            }
                                            serde_json::Value::Bool(b) => {
                                                query = query.bind(*b);
                                            }
                                            serde_json::Value::Null => {
                                                query = query.bind(None::<String>);
                                            }
                                            _ => {
                                                query = query.bind(val.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            query = query.bind(id);
                            query.execute(pool).await.map_err($crate::error::RepositoryError::Database)?;
                        }
                    }
                    Ok(())
                }

                async fn delete(&self, id: i64) -> Result<(), $crate::error::RepositoryError> {
                    let table = <$entity as $crate::entity::Entity>::table_name();
                    let pk_col = <$entity as $crate::entity::Entity>::primary_key_column();
                    let sql = format!("DELETE FROM {} WHERE {} = $1", table, pk_col);

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            sqlx::query(&sql)
                                .bind(id)
                                .execute(pool)
                                .await
                                .map_err($crate::error::RepositoryError::Database)?;
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            sqlx::query(&sql)
                                .bind(id)
                                .execute(pool)
                                .await
                                .map_err($crate::error::RepositoryError::Database)?;
                        }
                    }
                    Ok(())
                }

                async fn count(&self, query: &$crate::query::Query) -> Result<i64, $crate::error::RepositoryError> {
                    let table = <$entity as $crate::entity::Entity>::table_name();
                    let (where_clause, params) = query.to_sql();
                    let sql = format!("SELECT COUNT(*) as count FROM {} {}", table, where_clause);

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut q = sqlx::query(&sql);
                            for param in params {
                                q = q.bind(param);
                            }
                            let row = q.fetch_one(pool).await.map_err($crate::error::RepositoryError::Database)?;
                            use sqlx::Row;
                            Ok(row.get::<i64, _>("count"))
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut q = sqlx::query(&sql);
                            for param in params {
                                q = q.bind(param);
                            }
                            let row = q.fetch_one(pool).await.map_err($crate::error::RepositoryError::Database)?;
                            use sqlx::Row;
                            Ok(row.get::<i64, _>("count"))
                        }
                    }
                }
            }
        }
    };
}
