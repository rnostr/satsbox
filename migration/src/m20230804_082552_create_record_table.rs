use entity::record;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(record::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(record::Column::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(record::Column::UserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(record::Column::UserPubkey)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(record::Column::InvoiceId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(record::Column::Balance)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(record::Column::Change)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(record::Column::Source)
                            .string()
                            .not_null()
                            .default("".to_owned()),
                    )
                    .col(
                        ColumnDef::new(record::Column::Note)
                            .string()
                            .not_null()
                            .default("".to_owned()),
                    )
                    .col(
                        ColumnDef::new(record::Column::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(record::Entity).to_owned())
            .await
    }
}
