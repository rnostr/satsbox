// RUST_TEST_THREADS = "1"

use anyhow::Result;
use entity::invoice;
use migration::{Migrator, MigratorTrait};
use satsbox::{
    setting::{Fee, Lightning, Setting},
    AppState,
};

async fn create_test_state(lightning: Option<Lightning>, fee: Option<Fee>) -> Result<AppState> {
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_filename_override(".env.test");
    let mut setting = Setting::from_env("SATSBOX".to_owned())?;
    if let Some(lightning) = lightning {
        setting.lightning = lightning;
    }
    if let Some(fee) = fee {
        setting.fee = fee;
    }
    let state = AppState::from_setting(setting).await?;
    Migrator::fresh(state.service.conn()).await?;
    Ok(state)
}

#[actix_rt::test]
async fn info() -> Result<()> {
    let state = create_test_state(None, None).await?;
    let info = state.service.info().await?;
    assert_eq!(info.id.len(), 33);
    Ok(())
}

#[actix_rt::test]
async fn create_invoice() -> Result<()> {
    let pubkey = hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35")?;
    let expiry = 60 * 10;
    let memo = "test".to_owned();
    let msats = 2000_000;
    let source = "test".to_owned();
    let state = create_test_state(None, None).await?;
    let service = &state.service;
    let user = service.get_or_create_user(pubkey.clone()).await?;
    let invoice = service
        .create_invoice(&user, memo.clone(), msats, expiry, source.clone())
        .await?;

    assert_eq!(invoice.source, source);
    assert_eq!(invoice.status, invoice::Status::Unpaid);
    assert_eq!(invoice.amount, msats);
    assert_eq!(invoice.description, memo);
    assert_eq!(&invoice.service, service.name());
    assert_eq!(invoice.expiry, expiry);
    assert_eq!(invoice.expired_at, expiry + invoice.generated_at);
    Ok(())
}
