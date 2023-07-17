use crate::Result;
use lightning_client::{lightning::Info, Lightning};
use sea_orm::DbConn;

/// Lightning service
pub struct Service {
    lightning: Box<dyn Lightning + Sync + Send>,
    conn: DbConn,
}

impl Service {
    pub fn conn(&self) -> &DbConn {
        &self.conn
    }
    
    pub fn new(lightning: Box<dyn Lightning + Sync + Send>, conn: DbConn) -> Self {
        Self { lightning, conn }
    }

    pub async fn info(&self) -> Result<Info> {
        Ok(self.lightning.get_info().await?)
    }
}
