mod app;
mod hash;
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
    #[error("{0}")]
    Message(String),
}

impl actix_web::ResponseError for Error {}

pub type Result<T, E = Error> = core::result::Result<T, E>;
