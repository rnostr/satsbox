//! Rnostr cli

use clap::Parser;
use migration::{Migrator, MigratorTrait};
use satsbox::*;
use std::path::PathBuf;
use tracing::info;

/// Cli
#[derive(Debug, Parser)]
#[command(name = "satsbox", about = "satsbox dev server.", version)]
pub struct Cli {
    /// config file path
    #[arg(short = 'c', value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// fresh db
    #[arg(short = 'f')]
    pub fresh: bool,
}

struct TestUser {
    name: &'static str,
    password: &'static str,
    pubkey: Vec<u8>,
    balance: i64,
}

impl TestUser {
    fn new(name: &'static str, password: &'static str, pubkey: Vec<u8>, balance: i64) -> Self {
        Self {
            name,
            password,
            pubkey,
            balance,
        }
    }
}

#[actix_web::main]
async fn main() -> Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "DEBUG");
    }
    // try to load config from .dev.env
    let _ = dotenvy::from_filename(".dev.env");
    // println!("{:?}", std::env::vars().collect::<Vec<_>>());
    tracing_subscriber::fmt::init();

    let args = Cli::parse();
    let mut state: AppState = AppState::create(args.config, Some("SATSBOX".to_string())).await?;
    // public access
    state.setting.network.host = "0.0.0.0".to_string();

    if args.fresh {
        Migrator::fresh(state.service.db()).await?;
    } else {
        Migrator::up(state.service.db(), None).await?;
    }

    // create test user
    // https://hitony.com/nostrogen/

    let users = vec![
        // npub1fuvh5hz9tvyesqnrsrjlfy45j9dwj0zrzuzs4jy53kff850ge5sq6te9w6
        // nsec1cfnu2t9xpdxk25ufrtfqrm4a5whjrtwuakmzha3ye9pyzwsva4rqr0d73w
        // 4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd20
        // c267c52ca60b4d6553891ad201eebda3af21addcedb62bf624c942413a0ced46
        TestUser::new(
            "admin",
            "admin",
            hex::decode("4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd20")?,
            5_000_000_000,
        ),
        // npub1986chevdnac456ejlcfujwcaz9l4jgx34rvhdvxz3xemducca0xsze7gf9
        // nsec1epfzclddwnwqjcz9autjgh0xct0k8hkk5qtr02et0u84eknve4ssjtavml
        // 29f58be58d9f715a6b32fe13c93b1d117f5920d1a8d976b0c289b3b6f318ebcd
        // c8522c7dad74dc096045ef17245de6c2df63ded6a01637ab2b7f0f5cda6ccd61
        TestUser::new(
            "tester",
            "tester",
            hex::decode("29f58be58d9f715a6b32fe13c93b1d117f5920d1a8d976b0c289b3b6f318ebcd")?,
            2_000_000_000,
        ),
    ];

    for u in users {
        let exist = state.service.get_user(u.pubkey.clone()).await?.is_some();

        let user = state.service.get_or_create_user(u.pubkey.clone()).await?;
        state
            .service
            .update_user_name(user.id, Some(u.name.to_string()))
            .await?;
        state
            .service
            .update_user_password(user.id, Some(u.password.to_string()))
            .await?;

        if !exist {
            state
                .service
                .admin_adjust_user_balance(&user, u.balance, Some("for test".to_string()))
                .await?;
        }
        info!(
            "lndhub://{}:{}@http://127.0.0.1:8080/",
            hex::encode(u.pubkey),
            u.password
        );
    }

    info!("Start satsbox dev server");
    start(state).await?;
    info!("Server shutdown");
    Ok(())
}
