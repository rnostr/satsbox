#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error(transparent)]
    InvalidUri(#[from] tonic::codegen::http::uri::InvalidUri),
    #[error(transparent)]
    TransportError(#[from] tonic::transport::Error),
    #[error(transparent)]
    TonicStatus(#[from] tonic::Status),
    #[error(transparent)]
    OpensslErrorStack(#[from] openssl::error::ErrorStack),
    #[error(transparent)]
    Bolt11ParseError(#[from] lightning_invoice::Bolt11ParseError),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("{0}")]
    Message(String),
}

impl Error {
    pub fn from<E>(cause: E) -> Self
    where
        E: std::error::Error,
    {
        Self::Message(cause.to_string())
    }
}

pub type Result<T, E = Error> = core::result::Result<T, E>;

pub mod cln;
pub use cln::Cln;

pub mod lnd;
pub use lnd::Lnd;

pub mod lightning;
pub use lightning::Lightning;
