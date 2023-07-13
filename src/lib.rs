#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Lightning(#[from] lightning_client::Error),
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
