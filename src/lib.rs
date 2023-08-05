use std::time::{SystemTime, UNIX_EPOCH};

mod app;
mod hash;
mod jwt_auth;
pub mod lndhub;
mod service;
pub mod setting;

pub use {app::*, service::Service};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Lightning(#[from] lightning_client::Error),
    #[error(transparent)]
    DbErr(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error(transparent)]
    Notify(#[from] notify::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error("{0}")]
    Message(String),
    #[error("{0}")]
    Str(&'static str),
    #[error("{0}")]
    InvalidPayment(String),
    #[error("Unauthorized")]
    Unauthorized,
}

impl actix_web::ResponseError for Error {}

pub type Result<T, E = Error> = core::result::Result<T, E>;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
