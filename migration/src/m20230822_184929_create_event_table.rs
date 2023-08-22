use entity::event;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(event::Entity)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(event::Column::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(event::Column::EventId)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(event::Column::Status)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(event::Column::Json).text().not_null())
                    .col(
                        ColumnDef::new(event::Column::Message)
                            .text()
                            .not_null()
                            .default("".to_owned()),
                    )
                    .col(
                        ColumnDef::new(event::Column::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(event::Column::UpdatedAt)
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
                    .name("uq_event_id")
                    .col(event::Column::EventId)
                    .table(event::Entity)
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("uq_event_id").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(event::Entity).to_owned())
            .await
    }
}
