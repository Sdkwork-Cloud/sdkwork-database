use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpsError {
    #[error("spi error: {0}")]
    Spi(#[from] sdkwork_database_spi::SpiError),
    #[error("history error: {0}")]
    History(#[from] sdkwork_database_history::HistoryError),
    #[error("lifecycle error: {0}")]
    Lifecycle(#[from] sdkwork_database_lifecycle::LifecycleError),
    #[error("drift error: {0}")]
    Drift(#[from] sdkwork_database_drift::DriftError),
}
