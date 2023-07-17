use crate::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Info {
    #[serde(with = "hex::serde")]
    pub id: Vec<u8>,
}

/// the lightning trait for multiple backends
#[tonic::async_trait]
pub trait Lightning {
    async fn get_info(&self) -> Result<Info>;
}
