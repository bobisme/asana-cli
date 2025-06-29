use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Invalid task state: {0}")]
    InvalidTaskState(String),
    
    #[error("Invalid date format: {0}")]
    InvalidDate(String),
    
    #[error("Required field missing: {0}")]
    MissingField(String),
    
    #[error("Invalid identifier: {0}")]
    InvalidId(String),
}

pub type DomainResult<T> = Result<T, DomainError>;