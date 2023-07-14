pub use sea_orm_migration::prelude::*;

mod m20230714_035008_create_user_table;
mod m20230714_040219_create_invoice_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20230714_035008_create_user_table::Migration),
            Box::new(m20230714_040219_create_invoice_table::Migration),
        ]
    }
}
