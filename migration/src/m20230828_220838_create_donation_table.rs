use entity::donation;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(donation::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(donation::Column::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(donation::Column::UserId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(donation::Column::InvoiceId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(donation::Column::Amount)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(donation::Column::Message)
                            .text()
                            .not_null()
                            .default("".to_owned()),
                    )
                    .col(
                        ColumnDef::new(donation::Column::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(donation::Column::UpdatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("uq_vip_payment_invoice_id")
                    .col(donation::Column::InvoiceId)
                    .table(donation::Entity)
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("uq_vip_payment_invoice_id").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(donation::Entity).to_owned())
            .await
    }
}
