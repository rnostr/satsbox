use sea_orm::entity::prelude::*;

#[derive(EnumIter, DeriveActiveEnum, Debug, Clone, PartialEq, Eq)]
#[sea_orm(rs_type = "i32", db_type = "Integer")]
pub enum Status {
    Created = 0,
    Paid = 1,
}

/// donations

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "donations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub user_id: i32,

    pub invoice_id: i32,

    pub status: Status,

    pub amount: i64,

    pub paid_at: i64,

    #[sea_orm(column_type = "Text")]
    pub message: String,

    /// data create time
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
