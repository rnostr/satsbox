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
}

pub type Result<T, E = Error> = core::result::Result<T, E>;

pub mod cln;
pub use cln::Cln;

pub mod lnd;
pub use lnd::Lnd;

pub mod lightning;
pub use lightning::Lightning;
