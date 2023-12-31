//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.3

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Default)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub pubkey: Vec<u8>,
    /// user balance in msats
    pub balance: i64,
    /// Number of balances temporarily locked at the time of payment
    pub lock_amount: i64,
    /// custom unique username
    pub username: Option<String>,

    /// lndhub password
    pub password: Option<String>,

    /// donate amount
    pub donate_amount: i64,

    /// data create time
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
