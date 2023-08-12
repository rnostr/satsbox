use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod api;
mod app;
mod auth;
mod hash;
pub mod lndhub;
pub mod nwc;
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
    Hex(#[from] hex::FromHexError),
    #[error(transparent)]
    Secp256k1(#[from] nostr_sdk::prelude::secp256k1::Error),
    #[error(transparent)]
    NostrClient(#[from] nostr_sdk::client::Error),
    #[error(transparent)]
    NostrNip04(#[from] nostr_sdk::nips::nip04::Error),
    #[error(transparent)]
    NostrEventBuilder(#[from] nostr_sdk::event::builder::Error),
    #[error(transparent)]
    AddrParseError(#[from] std::net::AddrParseError),
    #[error(transparent)]
    Auth(#[from] auth::AuthError),
    #[error("{0}")]
    Message(String),
    #[error("{0}")]
    Str(&'static str),
    #[error("{0}")]
    InvalidPayment(String),
    #[error("Payment is being processed, please check the result later")]
    PaymentInProgress,
    #[error("The wallet does not have enough funds")]
    InsufficientBalance,
    #[error("Rate limiter exceeded")]
    RateLimited,
}

impl ResponseError for Error {
    fn status_code(&self) -> StatusCode {
        match self {
            Error::Auth(_) => StatusCode::UNAUTHORIZED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Creates full response for error.
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(json!({
            "error": true,
            "status_code": self.status_code().as_u16(),
            "message": self.to_string()
        }))
    }
}

pub type Result<T, E = Error> = core::result::Result<T, E>;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn sha256(s: impl AsRef<[u8]>) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(s);
    hasher.finalize().to_vec()
}
