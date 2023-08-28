use sea_orm::entity::prelude::*;

/// donations

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "donations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub user_id: i32,

    pub invoice_id: i32,

    pub amount: i64,

    #[sea_orm(column_type = "Text")]
    pub message: String,

    /// data create time
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
