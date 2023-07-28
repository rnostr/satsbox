//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.3

use sea_orm::entity::prelude::*;

#[derive(EnumIter, DeriveActiveEnum, Debug, Clone, PartialEq, Eq)]
#[sea_orm(rs_type = "u8", db_type = "SmallUnsigned")]
pub enum Type {
    Invoice = 0,
    Payment = 1,
}

#[derive(EnumIter, DeriveActiveEnum, Debug, Clone, PartialEq, Eq)]
#[sea_orm(rs_type = "u8", db_type = "SmallUnsigned")]
pub enum Status {
    Unpaid = 0,
    Paid = 1,
    /// failed when payment or invoice is expired
    Canceled = 2,
}

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
    pub r#type: Type,

    /// 0, unpaid, 1: paid, 2: canceled
    pub status: Status,

    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub payment_hash: Vec<u8>,
    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub payment_preimage: Vec<u8>,

    pub created_at: u64,
    pub expiry: u64,
    pub expired_at: u64,

    #[sea_orm(column_type = "Text")]
    pub description: String,
    #[sea_orm(column_type = "Text")]
    pub bolt11: String,

    /// invoice amount
    pub amount: u64,
    pub paid_at: u64,
    /// real paid amount
    pub paid_amount: u64,
    pub fee: u64,
    pub total: u64,
    /// Number of balances temporarily locked at the time of payment
    pub lock_amount: u64,
    /// user internal pay
    pub internal: bool,
    /// duplicate payment by external and internal
    pub duplicate: bool,
    pub service_fee: u64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
