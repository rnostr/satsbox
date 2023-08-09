//! Rnostr cli

use clap::Parser;
use migration::{Migrator, MigratorTrait};
use satsbox::*;
use std::path::PathBuf;
use tracing::info;

/// Cli
#[derive(Debug, Parser)]
#[command(name = "satsbox", about = "satsbox server.", version)]
pub struct Cli {
    /// config file path
    #[arg(short = 'c', value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[actix_web::main]
async fn main() -> Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }
    // try to load config from .env
    let _ = dotenvy::dotenv();
    // println!("{:?}", std::env::vars().collect::<Vec<_>>());
    tracing_subscriber::fmt::init();

    let args = Cli::parse();
    let state: AppState = AppState::create(args.config, Some("SATSBOX".to_string())).await?;
    Migrator::up(state.service.db(), None).await?;
    info!("Start satsbox server");
    start(state).await?;
    info!("Server shutdown");
    Ok(())
}
