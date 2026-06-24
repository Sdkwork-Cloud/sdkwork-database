use thiserror::Error;

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("spi error: {0}")]
    Spi(#[from] sdkwork_database_spi::SpiError),
    #[error("history error: {0}")]
    History(#[from] sdkwork_database_history::HistoryError),
    #[error("pool error: {0}")]
    Pool(#[from] sdkwork_database_sqlx::PoolError),
    #[error("migration error: {0}")]
    Migration(String),
    #[error("seed error: {0}")]
    Seed(String),
    #[error("state error: {0}")]
    State(String),
}
