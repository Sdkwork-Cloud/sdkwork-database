use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("io error: {0}")]
    Io(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("validation error: {0}")]
    Validation(String),
}
