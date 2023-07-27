//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.3

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "invoices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub user_id: i32,

    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub user_pubkey: Vec<u8>,

    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub payee: Vec<u8>,

    /// 0, invoice, 1: payment
    pub r#type: u8,

    /// 0, unpaid, 1: paid, 2: canceled
    pub status: u8,

    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub payment_hash: Vec<u8>,
    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub payment_preimage: Vec<u8>,

    pub created_at: u64,
    pub expiry: u64,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    #[sea_orm(column_type = "Text")]
    pub bolt11: String,

    /// invoice amount
    pub amount: u64,
    /// real paid amount
    pub paid_at: u64,
    pub paid_amount: u64,
    pub fee: u64,
    pub total: u64,
    /// Number of balances temporarily locked at the time of payment
    pub lock_amount: u64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
