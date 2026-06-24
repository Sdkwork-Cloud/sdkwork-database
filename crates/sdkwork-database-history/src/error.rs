use thiserror::Error;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("sql error: {0}")]
    Sql(String),
    #[error("migration error: {0}")]
    Migration(String),
    #[error("seed error: {0}")]
    Seed(String),
    #[error("state error: {0}")]
    State(String),
}
