use sea_orm::entity::prelude::*;

#[derive(EnumIter, DeriveActiveEnum, Debug, Clone, PartialEq, Eq)]
#[sea_orm(rs_type = "i32", db_type = "Integer")]
pub enum Status {
    Created = 0,
    Succeeded = 1,
    Failed = 2,
}

/// Nwc nostr events log

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub event_id: Vec<u8>,

    pub status: Status,

    /// origin event json
    #[sea_orm(column_type = "Text")]
    pub json: String,

    #[sea_orm(column_type = "Text")]
    pub message: String,

    /// data create time
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
