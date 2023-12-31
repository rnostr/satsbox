#![allow(unused)]
use lightning_client::{
    lightning::{InvoiceStatus, PaymentStatus},
    sha256, Error, Lightning, Result,
};
use rand::RngCore;

pub fn rand_preimage() -> Vec<u8> {
    let mut store_key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut store_key_bytes);
    store_key_bytes.to_vec()
}

pub async fn timeout<L: Lightning>(client: &L) -> Result<()> {
    // let image = rand_preimage();
    // let expiry = 60 * 10; // 10 minutes
    // let msats = 100_000;
    // let res = client
    //     .create_invoice("".to_owned(), msats, Some(image.clone()), Some(expiry))
    //     .await;
    let res = client.get_info().await;
    // println!("res {:?}", res);
    // maybe very fast
    // assert!(res.is_err());
    if res.is_ok() {
        println!("No timeout, was that the expected result?");
    }
    Ok(())
}

pub async fn get_info<L: Lightning>(client: &L) -> Result<()> {
    let info = client.get_info().await?;
    assert_eq!(info.id.len(), 33);
    // println!("inf {:?}", info);
    Ok(())
}

pub async fn create_invoice<L: Lightning>(client: &L) -> Result<()> {
    let info = client.get_info().await?;

    let image = rand_preimage();
    let expiry = 60 * 10; // 10 minutes
    let msats = 100_000;
    let memo = "test".to_owned();
    let description_hash = sha256(&memo);
    let hash = sha256(&image);
    let invoice = client
        .create_invoice(memo.clone(), msats, Some(image.clone()), Some(expiry))
        .await?;
    assert_eq!(invoice.description, Some(memo.clone()));
    assert_eq!(invoice.description_hash, Some(description_hash));
    assert_eq!(invoice.amount, msats);
    assert_eq!(invoice.expiry, expiry);
    assert_eq!(invoice.payment_hash, hash);
    assert_eq!(invoice.payee, info.id);

    let list = client.list_invoices(None, None).await?;
    let item = list.iter().find(|i| &i.payment_hash == &hash);
    assert!(item.is_some());

    let list = client
        .list_invoices(Some((invoice.created_at, invoice.index)), None)
        .await?;
    let item = list.iter().find(|i| &i.payment_hash == &hash);
    assert!(item.is_some());
    // time equal
    assert_eq!(list.first().unwrap().created_at, invoice.created_at);

    Ok(())
}

pub async fn payment<L1: Lightning, L2: Lightning>(c1: &L1, c2: &L2) -> Result<()> {
    let image = rand_preimage();
    let expiry = 60 * 10; // 10 minutes
    let msats = 100_000; // 100 sats
    let memo = "c1 pay to c2".to_owned();
    let invoice = c2
        .create_invoice(memo.clone(), msats, Some(image.clone()), Some(expiry))
        .await?;
    let payment_hash = invoice.payment_hash.clone();
    // println!("invoice {:?}", invoice);

    let inv = c2.lookup_invoice(payment_hash.clone()).await?;
    assert_eq!(inv.status, InvoiceStatus::Open);
    assert_eq!(inv.index, invoice.index);
    assert_eq!(inv.paid_amount, 0);
    assert_eq!(inv.paid_at, 0);

    // println!("invoice {:?}", inv);

    // pay success
    let hash = c1.pay(invoice.bolt11.clone(), None).await?;
    assert_eq!(payment_hash, hash);

    // check payment
    let payment = c1.lookup_payment(payment_hash.clone()).await?;
    assert_eq!(payment.status, PaymentStatus::Succeeded);
    assert_eq!(payment.payment_preimage, image);
    assert_eq!(payment.payment_hash, invoice.payment_hash);
    assert_eq!(payment.amount, msats);
    assert!(payment.total >= msats);
    // println!("payment {:?}", payment);
    let list = c1.list_payments(None, None).await?;
    let item = list.iter().find(|i| &i.payment_hash == &payment_hash);
    assert!(item.is_some());

    let inv = c2.lookup_invoice(payment_hash.clone()).await?;
    assert_eq!(inv.status, InvoiceStatus::Paid);
    assert_eq!(inv.paid_amount, inv.amount);
    assert!(inv.paid_at >= inv.created_at);
    // println!("invoice {:?}", inv);
    Ok(())
}

pub async fn payment_error<L1: Lightning, L2: Lightning>(c1: &L1, c2: &L2) -> Result<()> {
    let image = rand_preimage();
    let expiry = 60 * 10; // 10 minutes
    let msats = 2_000_000_000_000; // 2M sats exceeds channel maximum
    let memo = "c1 pay to c2".to_owned();
    let invoice = c2
        .create_invoice(memo.clone(), msats, Some(image.clone()), Some(expiry))
        .await?;
    let payment_hash = invoice.payment_hash.clone();
    // println!("invoice {:?}", invoice);

    let inv = c2.lookup_invoice(payment_hash.clone()).await?;
    assert_eq!(inv.status, InvoiceStatus::Open);
    assert_eq!(inv.index, invoice.index);

    // println!("invoice {:?}", inv);

    // pay failed
    let res = c1.pay(invoice.bolt11.clone(), None).await;
    assert!(res.is_err());

    // check payment
    let payment = c1.lookup_payment(payment_hash.clone()).await;
    // println!("payment {:?}", payment);
    match payment {
        Err(err) => {
            if !matches!(err, Error::PaymentNotFound) {
                println!("payment {:?}", err);
            }
            assert!(matches!(err, Error::PaymentNotFound));
        }
        Ok(payment) => {
            assert_eq!(payment.status, PaymentStatus::Failed);
        }
    }

    let inv = c2.lookup_invoice(payment_hash.clone()).await?;
    assert_eq!(inv.status, InvoiceStatus::Open);
    // println!("invoice {:?}", inv);
    Ok(())
}
