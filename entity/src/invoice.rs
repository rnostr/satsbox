//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.3

use sea_orm::entity::prelude::*;

#[derive(EnumIter, DeriveActiveEnum, Debug, Clone, PartialEq, Eq)]
#[sea_orm(rs_type = "i32", db_type = "Integer")]
pub enum Type {
    Invoice = 0,
    Payment = 1,
}

#[derive(EnumIter, DeriveActiveEnum, Debug, Clone, PartialEq, Eq)]
#[sea_orm(rs_type = "i32", db_type = "Integer")]
pub enum Status {
    Unpaid = 0,
    Paid = 1,
    /// failed when payment or invoice is expired
    Canceled = 2,
}

#[derive(EnumIter, DeriveActiveEnum, Debug, Clone, PartialEq, Eq)]
#[sea_orm(rs_type = "String", db_type = "String(Some(1))")]
pub enum Source {
    #[sea_orm(string_value = "test")]
    Test,
    #[sea_orm(string_value = "lndhub")]
    Lndhub,
    #[sea_orm(string_value = "lnurlp")]
    Lnurlp,
    #[sea_orm(string_value = "zaps")]
    Zaps,
    #[sea_orm(string_value = "nwc")]
    Nwc,
    #[sea_orm(string_value = "api")]
    Api,
}

impl Default for Source {
    fn default() -> Self {
        Self::Test
    }
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

    pub service: String,
    pub source: Source,

    pub index: i64,

    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub payment_hash: Vec<u8>,
    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub payment_preimage: Vec<u8>,

    /// data create time
    pub created_at: i64,
    pub updated_at: i64,

    /// invoice create time
    pub generated_at: i64,
    pub expiry: i64,
    pub expired_at: i64,

    #[sea_orm(column_type = "Text")]
    pub description: String,
    #[sea_orm(column_type = "Text")]
    pub bolt11: String,

    /// invoice amount
    pub amount: i64,
    pub paid_at: i64,
    /// real paid amount
    pub paid_amount: i64,
    pub fee: i64,

    /// payment amount + fee + service_fee
    pub total: i64,
    /// Number of balances temporarily locked at the time of payment
    pub lock_amount: i64,
    /// user internal pay
    pub internal: bool,
    /// duplicate payment by external and internal
    pub duplicate: bool,
    pub service_fee: i64,

    /// LUD-12 comment
    #[sea_orm(column_type = "Text")]
    pub comment: Option<String>,

    /// LUD-18 payerdata
    #[sea_orm(column_type = "Text")]
    pub payer: Option<String>,
    pub payer_name: Option<String>,
    pub payer_email: Option<String>,
    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub payer_pubkey: Option<Vec<u8>>,

    /// NIP-57 zap, zap event event is stored in the description field
    pub zap: bool,
    pub zap_status: i32,
    /// zap from user, nip26
    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub zap_from: Option<Vec<u8>>,
    /// zap to user pubkey
    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub zap_pubkey: Option<Vec<u8>>,
    /// zap to event id
    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub zap_event: Option<Vec<u8>>,
    /// NIP-57 zap receipt event
    #[sea_orm(column_type = "Text")]
    pub zap_receipt: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
