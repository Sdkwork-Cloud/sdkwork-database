use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriftError {
    #[error("spi error: {0}")]
    Spi(#[from] sdkwork_database_spi::SpiError),
    #[error("history error: {0}")]
    History(#[from] sdkwork_database_history::HistoryError),
    #[error("contract error: {0}")]
    Contract(#[from] sdkwork_database_contract::ContractError),
    #[error("introspect error: {0}")]
    Introspect(String),
}
