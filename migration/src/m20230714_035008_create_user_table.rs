use entity::user;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = Table::create()
            .table(user::Entity)
            .if_not_exists()
            .col(
                ColumnDef::new(user::Column::Id)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(user::Column::Pubkey)
                    .binary_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(user::Column::Balance)
                    .big_integer()
                    .not_null()
                    .default(0),
            )
            .col(
                ColumnDef::new(user::Column::LockAmount)
                    .big_integer()
                    .not_null()
                    .default(0),
            )
            .col(ColumnDef::new(user::Column::Username).string_len(50).null())
            .col(ColumnDef::new(user::Column::Password).string_len(50).null())
            .col(
                ColumnDef::new(user::Column::DonateAmount)
                    .big_integer()
                    .not_null()
                    .default(0),
            )
            .col(
                ColumnDef::new(user::Column::CreatedAt)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(user::Column::UpdatedAt)
                    .big_integer()
                    .not_null(),
            )
            .to_owned();

        manager.create_table(table).await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("uq_user_pubkey")
                    .col(user::Column::Pubkey)
                    .table(user::Entity)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("uq_user_username")
                    .col(user::Column::Username)
                    .table(user::Entity)
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("uq_user_pubkey").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("uq_user_username").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(user::Entity).to_owned())
            .await
    }
}
