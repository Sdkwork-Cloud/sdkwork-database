use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpiError {
    #[error("manifest error: {0}")]
    Manifest(String),
    #[error("asset error: {0}")]
    Asset(String),
    #[error("contract error: {0}")]
    Contract(String),
    #[error("migration error: {0}")]
    Migration(String),
    #[error("seed error: {0}")]
    Seed(String),
    #[error("drift error: {0}")]
    Drift(String),
    #[error("registry error: {0}")]
    Registry(String),
    #[error("lifecycle error: {0}")]
    Lifecycle(String),
}
