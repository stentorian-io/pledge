use crate::npm::NpmError;
use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PkgError {
    #[error("JSON error: {0}")]
    Json(#[from] json::JsonError),
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("NPM error: {0}")]
    Npm(#[from] Box<NpmError>),
    #[error("PatternError: {0}")]
    Pattern(#[from] glob::PatternError),
    #[error("Validation error: {0}")]
    Validation(String),
}

impl From<NpmError> for PkgError {
    fn from(error: NpmError) -> Self {
        PkgError::Npm(Box::new(error))
    }
}
