// RUST_TEST_THREADS = "1"

use anyhow::Result;
use entity::invoice;
use migration::{Migrator, MigratorTrait};
use satsbox::{
    setting::{Fee, Lightning, Setting},
    AppState,
};

async fn create_test_state(lightning: Option<Lightning>) -> Result<AppState> {
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_filename_override(".env.test");
    let mut setting = Setting::from_env("SATSBOX".to_owned())?;
    if let Some(lightning) = lightning {
        setting.lightning = lightning;
    }
    let state = AppState::from_setting(setting).await?;
    Ok(state)
}

async fn fresh_db(state: &AppState) -> Result<()> {
    Migrator::fresh(state.service.conn()).await?;
    Ok(())
}

#[actix_rt::test]
async fn info() -> Result<()> {
    let state = create_test_state(None).await?;
    fresh_db(&state).await?;
    let info = state.service.info().await?;
    assert_eq!(info.id.len(), 33);
    Ok(())
}

#[actix_rt::test]
async fn create_invoice() -> Result<()> {
    let pubkey = hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35")?;
    let expiry = 60 * 10;
    let memo = "test".to_owned();
    // big number test
    // 20 btc
    let msats = 2_000_000_000_000;

    let source = "test".to_owned();
    let state = create_test_state(None).await?;
    fresh_db(&state).await?;

    let service = &state.service;
    let user = service.get_or_create_user(pubkey.clone()).await?;
    let invoice = service
        .create_invoice(&user, memo.clone(), msats, expiry, source.clone())
        .await?;

    assert_eq!(invoice.source, source);
    assert_eq!(invoice.status, invoice::Status::Unpaid);
    assert_eq!(invoice.amount, msats as i64);
    assert_eq!(invoice.description, memo);
    assert_eq!(&invoice.service, service.name());
    assert_eq!(invoice.expiry, expiry as i64);
    assert_eq!(invoice.expired_at, expiry as i64 + invoice.generated_at);
    Ok(())
}

#[actix_rt::test]
async fn internal_payment() -> Result<()> {
    let payee_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35")?;
    let expiry = 60 * 10;
    let memo = "test".to_owned();
    // 2k sats
    let msats: i64 = 2_000_000;

    let source = "test".to_owned();
    let state = create_test_state(None).await?;
    fresh_db(&state).await?;

    let service = &state.service;
    let payee_user = service.get_or_create_user(payee_pubkey.clone()).await?;
    let payee_invoice = service
        .create_invoice(
            &payee_user,
            memo.clone(),
            msats as u64,
            expiry,
            source.clone(),
        )
        .await?;
    assert_eq!(payee_invoice.status, invoice::Status::Unpaid);

    let payer_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca36")?;
    let payer_user = service.get_or_create_user(payer_pubkey.clone()).await?;
    let payer_user = service.update_user_balance(&payer_user, 1000).await?;
    let fee = Fee {
        pay_limit_pct: 1.0,
        small_pay_limit_pct: 2.0,
        internal_pct: 0.5,
        service_pct: 0.3,
    };
    let res = service
        .pay(&payer_user, payee_invoice.bolt11.clone(), &fee, false)
        .await;
    // balance insufficient
    assert!(res.is_err());
    let balance = 5_000_000;
    let payer_user = service.update_user_balance(&payer_user, balance).await?;
    // println!("{:?}", payer_user);

    let payment = service
        .pay(&payer_user, payee_invoice.bolt11.clone(), &fee, false)
        .await?;
    let (internal_fee, service_fee) = fee.cal(msats as i64, true);

    let payee_invoice = service.get_invoice(payee_invoice.id).await?.unwrap();
    let payer_user = service.get_user(payer_pubkey).await?.unwrap();
    let payee_user = service.get_user(payee_pubkey).await?.unwrap();
    assert_eq!(payee_user.balance, msats);
    assert_eq!(
        payer_user.balance,
        balance - msats - internal_fee - service_fee
    );

    assert!(payment.internal);
    assert_eq!(payment.status, invoice::Status::Paid);
    assert_eq!(payment.fee, internal_fee);
    assert_eq!(payment.service_fee, service_fee);
    assert_eq!(payment.amount, msats);
    assert_eq!(payment.paid_amount, msats);

    assert!(payee_invoice.internal);
    assert_eq!(payee_invoice.status, invoice::Status::Paid);
    assert_eq!(payee_invoice.amount, msats);
    assert_eq!(payee_invoice.paid_amount, msats);

    assert_eq!(payment.payment_preimage, payee_invoice.payment_preimage);

    // repeat pay
    let res = service
        .pay(&payer_user, payee_invoice.bolt11.clone(), &fee, false)
        .await;
    assert!(res.is_err());
    assert!(res.err().unwrap().to_string().contains("closed"));

    Ok(())
}

#[actix_rt::test]
async fn external_payment() -> Result<()> {
    // pay(Lightning::Cln, Lightning::Lnd).await?;
    Ok(())
}

async fn pay(from: Lightning, to: Lightning) -> Result<()> {
    let payee_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35")?;
    let expiry = 60 * 10;
    let memo = "test".to_owned();
    // 2k sats
    let msats: i64 = 2_000_000;

    let source = "test".to_owned();
    let state = create_test_state(None).await?;
    fresh_db(&state).await?;

    let service = &state.service;
    let payee_user = service.get_or_create_user(payee_pubkey.clone()).await?;
    let payee_invoice = service
        .create_invoice(
            &payee_user,
            memo.clone(),
            msats as u64,
            expiry,
            source.clone(),
        )
        .await?;
    assert_eq!(payee_invoice.status, invoice::Status::Unpaid);

    let payer_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca36")?;
    let payer_user = service.get_or_create_user(payer_pubkey.clone()).await?;
    let payer_user = service.update_user_balance(&payer_user, 1000).await?;
    let fee = Fee {
        pay_limit_pct: 1.0,
        small_pay_limit_pct: 2.0,
        internal_pct: 0.5,
        service_pct: 0.3,
    };
    let res = service
        .pay(&payer_user, payee_invoice.bolt11.clone(), &fee, false)
        .await;
    // balance insufficient
    assert!(res.is_err());
    let balance = 5_000_000;
    let payer_user = service.update_user_balance(&payer_user, balance).await?;
    // println!("{:?}", payer_user);

    let payment = service
        .pay(&payer_user, payee_invoice.bolt11.clone(), &fee, false)
        .await?;
    let (internal_fee, service_fee) = fee.cal(msats as i64, true);

    let payee_invoice = service.get_invoice(payee_invoice.id).await?.unwrap();
    let payer_user = service.get_user(payer_pubkey).await?.unwrap();
    let payee_user = service.get_user(payee_pubkey).await?.unwrap();
    assert_eq!(payee_user.balance, msats);
    assert_eq!(
        payer_user.balance,
        balance - msats - internal_fee - service_fee
    );

    assert!(payment.internal);
    assert_eq!(payment.status, invoice::Status::Paid);
    assert_eq!(payment.fee, internal_fee);
    assert_eq!(payment.service_fee, service_fee);
    assert_eq!(payment.amount, msats);
    assert_eq!(payment.paid_amount, msats);

    assert!(payee_invoice.internal);
    assert_eq!(payee_invoice.status, invoice::Status::Paid);
    assert_eq!(payee_invoice.amount, msats);
    assert_eq!(payee_invoice.paid_amount, msats);

    assert_eq!(payment.payment_preimage, payee_invoice.payment_preimage);

    // repeat pay
    let res = service
        .pay(&payer_user, payee_invoice.bolt11.clone(), &fee, false)
        .await;
    assert!(res.is_err());
    assert!(res.err().unwrap().to_string().contains("closed"));

    Ok(())
}
