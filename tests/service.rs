// RUST_TEST_THREADS=1 cargo test --test service -- --nocapture

use anyhow::Result;
use entity::invoice;
use satsbox::{
    now,
    setting::{Fee, Lightning},
    InvoiceExtra,
};
use std::time::Duration;
use tokio::time::sleep;
use util::create_test_state2;

mod util;

// async fn fresh_db(state: &AppState) -> Result<()> {
//     Migrator::fresh(state.service.conn()).await?;
//     Ok(())
// }

#[tokio::test]
async fn info() -> Result<()> {
    let state = create_test_state2(None).await?;
    let info = state.service.info().await?;
    assert_eq!(info.id.len(), 33);
    Ok(())
}

#[tokio::test]
async fn create_invoice() -> Result<()> {
    let pubkey = hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35")?;
    let expiry = 60 * 10;
    let memo = "test".to_owned();
    // big number test
    // 20 btc
    let msats = 2_000_000_000_000;

    let source = entity::invoice::Source::Test;
    let state = create_test_state2(None).await?;

    let service = &state.service;
    let user = service.get_or_create_user(pubkey.clone()).await?;
    let invoice = service
        .create_invoice(
            &user,
            memo.clone(),
            msats,
            expiry,
            InvoiceExtra::new(source.clone()),
        )
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

#[tokio::test]
async fn internal_payment() -> Result<()> {
    let payee_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35")?;
    let expiry = 60 * 10;
    let memo = "test".to_owned();
    // 2k sats
    let msats: i64 = 2_000_000;

    let source = entity::invoice::Source::Test;
    let state = create_test_state2(None).await?;

    let service = &state.service;
    let payee_user = service.get_or_create_user(payee_pubkey.clone()).await?;
    let payee_invoice = service
        .create_invoice(
            &payee_user,
            memo.clone(),
            msats as u64,
            expiry,
            InvoiceExtra::new(source),
        )
        .await?;
    assert_eq!(payee_invoice.status, invoice::Status::Unpaid);

    let payer_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca36")?;
    let payer_user = service.get_or_create_user(payer_pubkey.clone()).await?;

    let fee = Fee {
        pay_limit_pct: 1.0,
        small_pay_limit_pct: 2.0,
        internal_pct: 0.5,
        service_pct: 0.3,
    };
    let res = service
        .pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false,
        )
        .await;
    // balance insufficient
    assert!(res.is_err());
    let balance = 5_000_000;
    let payer_user = service
        .admin_adjust_user_balance(&payer_user, balance, None)
        .await?;
    // println!("{:?}", payer_user);

    // let payment = service
    //     .pay(&payer_user, payee_invoice.bolt11.clone(), &fee, entity::invoice::Source::Test, false)
    //     .await?;
    let res = tokio::join!(
        service.pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false
        ),
        service.pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false
        )
    );
    let (_err, payment) = match res {
        (Err(err), Ok(payment)) => (err, payment),
        (Ok(payment), Err(err)) => (err, payment),
        _ => {
            panic!("repeated payment")
        }
    };

    let (internal_fee, service_fee) = fee.cal(msats, true);

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
    assert_eq!(payment.total, msats + internal_fee + service_fee);

    assert!(payee_invoice.internal);
    assert_eq!(payee_invoice.status, invoice::Status::Paid);
    assert_eq!(payee_invoice.amount, msats);
    assert_eq!(payee_invoice.paid_amount, msats);

    assert_eq!(payment.payment_preimage, payee_invoice.payment_preimage);

    // repeat pay
    let res = service
        .pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false,
        )
        .await;
    assert!(res.is_err());
    assert!(res.err().unwrap().to_string().contains("closed"));

    Ok(())
}

#[tokio::test]
async fn self_payment() -> Result<()> {
    let pubkey = hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35")?;
    let expiry = 60 * 10;
    let memo = "test".to_owned();
    // 2k sats
    let msats: i64 = 2_000_000;

    let source = entity::invoice::Source::Test;
    let mut state = create_test_state2(None).await?;
    state.service.self_payment = true;

    let service = &state.service;
    let user = service.get_or_create_user(pubkey.clone()).await?;
    let invoice = service
        .create_invoice(
            &user,
            memo.clone(),
            msats as u64,
            expiry,
            InvoiceExtra::new(source),
        )
        .await?;
    assert_eq!(invoice.status, invoice::Status::Unpaid);

    let fee = Fee {
        pay_limit_pct: 1.0,
        small_pay_limit_pct: 2.0,
        internal_pct: 0.5,
        service_pct: 0.3,
    };

    let balance = 5_000_000;
    let user = service
        .admin_adjust_user_balance(&user, balance, None)
        .await?;

    let res = tokio::join!(
        service.pay(
            &user,
            invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false
        ),
        service.pay(
            &user,
            invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false
        )
    );
    let (_err, payment) = match res {
        (Err(err), Ok(payment)) => (err, payment),
        (Ok(payment), Err(err)) => (err, payment),
        _ => {
            panic!("repeated payment")
        }
    };

    let (internal_fee, service_fee) = fee.cal(msats, true);

    let invoice = service.get_invoice(invoice.id).await?.unwrap();
    let user = service.get_user(pubkey).await?.unwrap();

    assert_eq!(user.balance, balance - internal_fee - service_fee);

    assert!(payment.internal);
    assert_eq!(payment.status, invoice::Status::Paid);
    assert_eq!(payment.fee, internal_fee);
    assert_eq!(payment.service_fee, service_fee);
    assert_eq!(payment.amount, msats);
    assert_eq!(payment.paid_amount, msats);
    assert_eq!(payment.total, msats + internal_fee + service_fee);

    assert!(invoice.internal);
    assert_eq!(invoice.status, invoice::Status::Paid);
    assert_eq!(invoice.amount, msats);
    assert_eq!(invoice.paid_amount, msats);

    assert_eq!(payment.payment_preimage, invoice.payment_preimage);

    Ok(())
}

#[tokio::test]
async fn external_payment_cln_to_lnd_nosync() -> Result<()> {
    pay(Lightning::Cln, Lightning::Lnd, false).await
}

#[tokio::test]
async fn external_payment_cln_to_lnd_sync() -> Result<()> {
    pay(Lightning::Cln, Lightning::Lnd, true).await
}

#[tokio::test]
async fn external_payment_lnd_to_cln_nosync() -> Result<()> {
    pay(Lightning::Lnd, Lightning::Cln, false).await
}

#[tokio::test]
async fn external_payment_lnd_to_cln_sync() -> Result<()> {
    pay(Lightning::Lnd, Lightning::Cln, true).await
}

async fn pay(payer: Lightning, payee: Lightning, test_sync: bool) -> Result<()> {
    let payee_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35")?;
    let expiry = 60 * 10;
    let memo = "test".to_owned();
    // 2k sats
    let msats: i64 = 2_000_000;

    let source = entity::invoice::Source::Test;
    // create_test_state2 will refresh db
    let payee_state = create_test_state2(Some(payee)).await?;
    let payer_state = create_test_state2(Some(payer)).await?;

    let payee_service = &payee_state.service;
    let payee_user = payee_service
        .get_or_create_user(payee_pubkey.clone())
        .await?;

    let payer_service = &payer_state.service;
    let payer_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca36")?;
    let payer_user = payer_service
        .get_or_create_user(payer_pubkey.clone())
        .await?;

    let payee_invoice = payee_service
        .create_invoice(
            &payee_user,
            memo.clone(),
            msats as u64,
            expiry,
            InvoiceExtra::new(source),
        )
        .await?;
    assert_eq!(payee_invoice.status, invoice::Status::Unpaid);

    let fee = Fee {
        pay_limit_pct: 1.0,
        small_pay_limit_pct: 2.0,
        internal_pct: 0.5,
        service_pct: 0.3,
    };
    let res = payer_service
        .pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false,
        )
        .await;
    // balance insufficient
    assert!(res.is_err());
    let balance = 5_000_000;
    let payer_user = payer_service
        .admin_adjust_user_balance(&payer_user, balance, None)
        .await?;
    // println!("{:?}", payer_user);

    // let payment = payer_service
    //     .pay(&payer_user, payee_invoice.bolt11.clone(), &fee, entity::invoice::Source::Test, false)
    //     .await?;
    let res = tokio::join!(
        payer_service.pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            test_sync
        ),
        payer_service.pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            test_sync
        )
    );

    let (_err, payment) = match res {
        (Err(err), Ok(payment)) => (err, payment),
        (Ok(payment), Err(err)) => (err, payment),
        _ => {
            panic!("repeated payment")
        }
    };
    let mut payment = payment;
    if test_sync {
        // sync
        payer_service.sync_payments(None).await?;
        payment = payer_service.get_invoice(payment.id).await?.unwrap();
    }

    // println!("payment {:?} {:?}", _err, payment);
    sleep(Duration::from_secs(1)).await;
    let count = payee_service.sync_invoices(now() - 60).await?;
    assert_eq!(count, 1);

    let (_max_fee, service_fee) = fee.cal(msats, false);
    let real_fee = 0;

    let payee_invoice = payee_service.get_invoice(payee_invoice.id).await?.unwrap();
    let payer_user = payer_service.get_user(payer_pubkey).await?.unwrap();
    let payee_user = payee_service.get_user(payee_pubkey).await?.unwrap();
    assert_eq!(payee_user.balance, msats);
    assert_eq!(payer_user.balance, balance - msats - real_fee - service_fee);

    assert!(!payment.internal);
    assert_eq!(payment.status, invoice::Status::Paid);
    assert_eq!(payment.fee, real_fee);
    assert_eq!(payment.service_fee, service_fee);
    assert_eq!(payment.amount, msats);
    assert_eq!(payment.paid_amount, msats);
    assert_eq!(payment.total, msats + real_fee + service_fee);

    assert!(!payee_invoice.internal);
    assert_eq!(payee_invoice.status, invoice::Status::Paid);
    assert_eq!(payee_invoice.amount, msats);
    assert_eq!(payee_invoice.paid_amount, msats);

    assert_eq!(payment.payment_preimage, payee_invoice.payment_preimage);

    // repeat pay
    let res = payer_service
        .pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false,
        )
        .await;
    assert!(res.is_err());
    assert!(res.err().unwrap().to_string().contains("exists"));

    Ok(())
}

#[tokio::test]
async fn duplicate_payment() -> Result<()> {
    let payee_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35")?;
    let expiry = 60 * 10;
    let memo = "test".to_owned();
    // 2k sats
    let msats: i64 = 2_000_000;

    let source = entity::invoice::Source::Test;
    let state = create_test_state2(Some(Lightning::Cln)).await?;
    let payer_state = create_test_state2(Some(Lightning::Lnd)).await?;

    let service = &state.service;
    let payee_user = service.get_or_create_user(payee_pubkey.clone()).await?;
    let payee_invoice = service
        .create_invoice(
            &payee_user,
            memo.clone(),
            msats as u64,
            expiry,
            InvoiceExtra::new(source),
        )
        .await?;
    assert_eq!(payee_invoice.status, invoice::Status::Unpaid);

    let payer_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca36")?;
    let payer_user = service.get_or_create_user(payer_pubkey.clone()).await?;
    let balance: i64 = 5_000_000;

    let payer_user = service
        .admin_adjust_user_balance(&payer_user, balance, None)
        .await?;
    let fee = Fee {
        pay_limit_pct: 1.0,
        small_pay_limit_pct: 2.0,
        internal_pct: 0.5,
        service_pct: 0.3,
    };

    // let payment = service
    //     .pay(&payer_user, payee_invoice.bolt11.clone(), &fee, entity::invoice::Source::Test, false)
    //     .await?;
    let res = tokio::join!(
        service.pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false
        ),
        service.pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false
        )
    );
    let (_err, payment) = match res {
        (Err(err), Ok(payment)) => (err, payment),
        (Ok(payment), Err(err)) => (err, payment),
        _ => {
            panic!("repeated payment")
        }
    };

    // repeat pay
    payer_state
        .service
        .lightning()
        .pay(payee_invoice.bolt11.clone(), None)
        .await?;

    sleep(Duration::from_secs(1)).await;
    let count = service.sync_invoices(now() - 60).await?;
    assert_eq!(count, 1);

    let (internal_fee, service_fee) = fee.cal(msats, true);

    let payee_invoice = service.get_invoice(payee_invoice.id).await?.unwrap();
    let payer_user = service.get_user(payer_pubkey).await?.unwrap();
    let payee_user = service.get_user(payee_pubkey).await?.unwrap();
    assert_eq!(payee_user.balance, msats * 2);
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
    assert_eq!(payee_invoice.paid_amount, msats * 2);
    assert!(payee_invoice.duplicate);

    assert_eq!(payment.payment_preimage, payee_invoice.payment_preimage);

    // repeat pay
    let res = service
        .pay(
            &payer_user,
            payee_invoice.bolt11.clone(),
            &fee,
            entity::invoice::Source::Test,
            false,
        )
        .await;
    assert!(res.is_err());
    assert!(res.err().unwrap().to_string().contains("closed"));

    Ok(())
}
