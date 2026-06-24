//! Advanced database operations including batch operations and transactions.

use async_trait::async_trait;
use sdkwork_database_sqlx::DatabasePool;

use crate::entity::Entity;
use crate::error::RepositoryError;

/// Batch operations for bulk database operations.
///
/// This trait provides methods for inserting, updating, and deleting
/// multiple entities at once, which is more efficient than individual operations.
///
/// # Example
///
/// ```rust,ignore
/// use sdkwork_database_repository::prelude::*;
/// use sdkwork_database_repository::batch::BatchOperations;
///
/// let users = vec![user1, user2, user3];
/// repo.insert_batch(&users).await?;
/// ```
#[async_trait]
pub trait BatchOperations<E: Entity>: Send + Sync {
    /// Get the database pool.
    fn pool(&self) -> &DatabasePool;

    /// Insert multiple entities at once.
    ///
    /// This is more efficient than calling `insert` multiple times
    /// because it batches the operations into a single transaction.
    async fn insert_batch(&self, entities: &[E]) -> Result<(), RepositoryError>;

    /// Update multiple entities at once.
    async fn update_batch(&self, entities: &[E]) -> Result<(), RepositoryError>;

    /// Delete multiple entities by their IDs.
    async fn delete_batch(&self, ids: &[i64]) -> Result<(), RepositoryError>;

    /// Insert or update multiple entities (upsert).
    ///
    /// If an entity with the same primary key exists, it will be updated.
    /// Otherwise, it will be inserted.
    async fn upsert_batch(&self, entities: &[E]) -> Result<(), RepositoryError>;
}

/// Transaction management for database operations.
///
/// This trait provides methods for executing multiple operations
/// within a single transaction, ensuring atomicity.
///
/// # Example
///
/// ```rust,ignore
/// use sdkwork_database_repository::prelude::*;
/// use sdkwork_database_repository::transaction::TransactionOperations;
///
/// repo.execute_in_transaction(|repo| async move {
///     repo.insert(&user).await?;
///     repo.insert(&profile).await?;
///     Ok(())
/// }).await?;
/// ```
#[async_trait]
pub trait TransactionOperations<E: Entity>: Send + Sync + Sized {
    /// Get the database pool.
    fn pool(&self) -> &DatabasePool;

    /// Execute operations within a transaction.
    ///
    /// If any operation fails, the entire transaction is rolled back.
    /// If all operations succeed, the transaction is committed.
    async fn execute_in_transaction<F, Fut, R>(&self, operations: F) -> Result<R, RepositoryError>
    where
        F: FnOnce(Self) -> Fut + Send,
        Fut: std::future::Future<Output = Result<R, RepositoryError>> + Send,
        R: Send;

    /// Execute operations within a transaction with savepoints.
    ///
    /// This allows partial rollbacks within the transaction.
    async fn execute_with_savepoint<F, Fut, R>(&self, operations: F) -> Result<R, RepositoryError>
    where
        F: FnOnce(Self) -> Fut + Send,
        Fut: std::future::Future<Output = Result<R, RepositoryError>> + Send,
        R: Send;
}

/// Macro to implement BatchOperations for a Repository.
///
/// # Example
///
/// ```rust,ignore
/// use sdkwork_database_repository::prelude::*;
/// use sdkwork_database_repository::{impl_entity, impl_repository, impl_batch_operations};
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
/// impl_batch_operations!(User);
/// ```
#[macro_export]
macro_rules! impl_batch_operations {
    ($entity:ident) => {
        paste::paste! {
            #[async_trait::async_trait]
            impl $crate::batch::BatchOperations<$entity> for [<$entity Repository>] {
                fn pool(&self) -> &sdkwork_database_sqlx::DatabasePool {
                    &self.pool
                }

                async fn insert_batch(&self, entities: &[$entity]) -> Result<(), $crate::error::RepositoryError> {
                    if entities.is_empty() {
                        return Ok(());
                    }

                    let table = <$entity as $crate::entity::Entity>::table_name();
                    let columns = <$entity as $crate::entity::Entity>::columns();

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;

                            for entity in entities {
                                let placeholders: Vec<String> = columns.iter().enumerate().map(|(i, _)| format!("${}", i + 1)).collect();
                                let sql = format!(
                                    "INSERT INTO {} ({}) VALUES ({})",
                                    table,
                                    columns.join(", "),
                                    placeholders.join(", ")
                                );

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
                                query.execute(&mut *tx).await.map_err($crate::error::RepositoryError::Database)?;
                            }

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;

                            for entity in entities {
                                let placeholders: Vec<String> = columns.iter().enumerate().map(|(i, _)| format!("${}", i + 1)).collect();
                                let sql = format!(
                                    "INSERT INTO {} ({}) VALUES ({})",
                                    table,
                                    columns.join(", "),
                                    placeholders.join(", ")
                                );

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
                                query.execute(&mut *tx).await.map_err($crate::error::RepositoryError::Database)?;
                            }

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                        }
                    }
                    Ok(())
                }

                async fn update_batch(&self, entities: &[$entity]) -> Result<(), $crate::error::RepositoryError> {
                    if entities.is_empty() {
                        return Ok(());
                    }

                    let table = <$entity as $crate::entity::Entity>::table_name();
                    let columns = <$entity as $crate::entity::Entity>::columns();

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;

                            for entity in entities {
                                let id = entity.primary_key();
                                let set_clauses: Vec<String> = columns.iter().enumerate()
                                    .filter(|(_, col)| **col != "id")
                                    .map(|(i, col)| format!("{} = ${}", col, i + 1))
                                    .collect();

                                let sql = format!(
                                    "UPDATE {} SET {} WHERE id = ${}",
                                    table,
                                    set_clauses.join(", "),
                                    columns.len() + 1
                                );

                                let mut query = sqlx::query(&sql);
                                let json = entity.to_json();
                                for col in columns {
                                    if *col != "id" {
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
                                query.execute(&mut *tx).await.map_err($crate::error::RepositoryError::Database)?;
                            }

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;

                            for entity in entities {
                                let id = entity.primary_key();
                                let set_clauses: Vec<String> = columns.iter().enumerate()
                                    .filter(|(_, col)| **col != "id")
                                    .map(|(i, col)| format!("{} = ${}", col, i + 1))
                                    .collect();

                                let sql = format!(
                                    "UPDATE {} SET {} WHERE id = ${}",
                                    table,
                                    set_clauses.join(", "),
                                    columns.len() + 1
                                );

                                let mut query = sqlx::query(&sql);
                                let json = entity.to_json();
                                for col in columns {
                                    if *col != "id" {
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
                                query.execute(&mut *tx).await.map_err($crate::error::RepositoryError::Database)?;
                            }

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                        }
                    }
                    Ok(())
                }

                async fn delete_batch(&self, ids: &[i64]) -> Result<(), $crate::error::RepositoryError> {
                    if ids.is_empty() {
                        return Ok(());
                    }

                    let table = <$entity as $crate::entity::Entity>::table_name();

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;

                            for id in ids {
                                let sql = format!("DELETE FROM {} WHERE id = $1", table);
                                sqlx::query(&sql)
                                    .bind(id)
                                    .execute(&mut *tx)
                                    .await
                                    .map_err($crate::error::RepositoryError::Database)?;
                            }

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;

                            for id in ids {
                                let sql = format!("DELETE FROM {} WHERE id = $1", table);
                                sqlx::query(&sql)
                                    .bind(id)
                                    .execute(&mut *tx)
                                    .await
                                    .map_err($crate::error::RepositoryError::Database)?;
                            }

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                        }
                    }
                    Ok(())
                }

                async fn upsert_batch(&self, entities: &[$entity]) -> Result<(), $crate::error::RepositoryError> {
                    if entities.is_empty() {
                        return Ok(());
                    }

                    let table = <$entity as $crate::entity::Entity>::table_name();
                    let columns = <$entity as $crate::entity::Entity>::columns();

                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;

                            for entity in entities {
                                let placeholders: Vec<String> = columns.iter().enumerate().map(|(i, _)| format!("${}", i + 1)).collect();
                                let update_clauses: Vec<String> = columns.iter()
                                    .filter(|col| **col != "id")
                                    .map(|col| format!("{} = excluded.{}", col, col))
                                    .collect();

                                let sql = format!(
                                    "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT(id) DO UPDATE SET {}",
                                    table,
                                    columns.join(", "),
                                    placeholders.join(", "),
                                    update_clauses.join(", ")
                                );

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
                                query.execute(&mut *tx).await.map_err($crate::error::RepositoryError::Database)?;
                            }

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;

                            for entity in entities {
                                let placeholders: Vec<String> = columns.iter().enumerate().map(|(i, _)| format!("${}", i + 1)).collect();
                                let update_clauses: Vec<String> = columns.iter()
                                    .filter(|col| **col != "id")
                                    .map(|col| format!("{} = excluded.{}", col, col))
                                    .collect();

                                let sql = format!(
                                    "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT(id) DO UPDATE SET {}",
                                    table,
                                    columns.join(", "),
                                    placeholders.join(", "),
                                    update_clauses.join(", ")
                                );

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
                                query.execute(&mut *tx).await.map_err($crate::error::RepositoryError::Database)?;
                            }

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                        }
                    }
                    Ok(())
                }
            }
        }
    };
}

/// Macro to implement TransactionOperations for a Repository.
///
/// # Example
///
/// ```rust,ignore
/// use sdkwork_database_repository::prelude::*;
/// use sdkwork_database_repository::{impl_entity, impl_repository, impl_transaction_operations};
///
/// impl_entity!(User, "users", id, [id, name, email]);
/// impl_repository!(User);
/// impl_transaction_operations!(User);
/// ```
#[macro_export]
macro_rules! impl_transaction_operations {
    ($entity:ident) => {
        paste::paste! {
            #[async_trait::async_trait]
            impl $crate::transaction::TransactionOperations<$entity> for [<$entity Repository>] {
                fn pool(&self) -> &sdkwork_database_sqlx::DatabasePool {
                    &self.pool
                }

                async fn execute_in_transaction<F, Fut, R>(&self, operations: F) -> Result<R, $crate::error::RepositoryError>
                where
                    F: FnOnce(Self) -> Fut + Send,
                    Fut: std::future::Future<Output = Result<R, $crate::error::RepositoryError>> + Send,
                    R: Send,
                {
                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;
                            let tx_pool = sdkwork_database_sqlx::DatabasePool::Sqlite(tx.clone(), self.pool.context().clone());
                            let tx_repo = Self::new(tx_pool);

                            let result = operations(tx_repo).await?;

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                            Ok(result)
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;
                            let tx_pool = sdkwork_database_sqlx::DatabasePool::Postgres(tx.clone(), self.pool.context().clone());
                            let tx_repo = Self::new(tx_pool);

                            let result = operations(tx_repo).await?;

                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                            Ok(result)
                        }
                    }
                }

                async fn execute_with_savepoint<F, Fut, R>(&self, operations: F) -> Result<R, $crate::error::RepositoryError>
                where
                    F: FnOnce(Self) -> Fut + Send,
                    Fut: std::future::Future<Output = Result<R, $crate::error::RepositoryError>> + Send,
                    R: Send,
                {
                    match &self.pool {
                        sdkwork_database_sqlx::DatabasePool::Sqlite(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;
                            let sp = tx.begin().await.map_err($crate::error::RepositoryError::Database)?;
                            let sp_pool = sdkwork_database_sqlx::DatabasePool::Sqlite(sp.clone(), self.pool.context().clone());
                            let sp_repo = Self::new(sp_pool);

                            let result = operations(sp_repo).await?;

                            sp.commit().await.map_err($crate::error::RepositoryError::Database)?;
                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                            Ok(result)
                        }
                        sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) => {
                            let mut tx = pool.begin().await.map_err($crate::error::RepositoryError::Database)?;
                            let sp = tx.begin().await.map_err($crate::error::RepositoryError::Database)?;
                            let sp_pool = sdkwork_database_sqlx::DatabasePool::Postgres(sp.clone(), self.pool.context().clone());
                            let sp_repo = Self::new(sp_pool);

                            let result = operations(sp_repo).await?;

                            sp.commit().await.map_err($crate::error::RepositoryError::Database)?;
                            tx.commit().await.map_err($crate::error::RepositoryError::Database)?;
                            Ok(result)
                        }
                    }
                }
            }
        }
    };
}
