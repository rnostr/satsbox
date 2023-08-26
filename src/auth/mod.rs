mod jwt;
mod nostr;

pub use jwt::*;
pub use nostr::*;

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    // #[error(transparent)]
    // Io(#[from] std::io::Error),
    #[error(transparent)]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error("decode error")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("decode error")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
    #[error("invalid event")]
    Event(#[from] nostr_sdk::nostr::event::Error),
    #[error("{0}")]
    InvalidEvent(&'static str),
    #[error("{0}")]
    Invalid(&'static str),
    // #[error("Unauthorized")]
    // Unauthorized,
    #[error("Pubkey not in whitelist")]
    Whitelist,
}
