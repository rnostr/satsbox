use entity::invoice;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(invoice::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(invoice::Column::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(invoice::Column::Destination)
                            .binary_len(33)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(invoice::Column::Amount)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(invoice::Column::Timestamp)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(invoice::Column::Expiry).integer().null())
                    .col(ColumnDef::new(invoice::Column::Description).text().null())
                    .col(ColumnDef::new(invoice::Column::Request).text().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(invoice::Entity).to_owned())
            .await
    }
}
