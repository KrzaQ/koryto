//! Operations shared by the API and the MCP server: they check the household,
//! resolve zones and days, apply the domain rules and hand the result to the
//! database. Errors carry enough shape for both an HTTP status and an MCP
//! error message.

pub mod day;
pub mod entries;
pub mod scope;
pub mod stats;
pub mod time;

use crate::db::DbError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Db(sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl From<DbError> for AppError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::NotFound => Self::NotFound,
            DbError::Conflict(m) => Self::Conflict(m),
            DbError::Sqlx(e) => Self::Db(e),
        }
    }
}

pub fn bad(msg: impl Into<String>) -> AppError {
    AppError::BadRequest(msg.into())
}
