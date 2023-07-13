use crate::Result;

#[derive(Debug)]
pub struct Info {
    pub id: Vec<u8>,
}
/// the lightning trait for multiple backends
#[tonic::async_trait]
pub trait Lightning {
    async fn get_info(&mut self) -> Result<Info>;
}
