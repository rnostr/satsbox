use actix_rt::time::sleep;
use actix_web::{test::init_service, web};
use anyhow::Result;
use lightning_client::{lightning::Invoice, sha256};
use nostr_sdk::{
    secp256k1::XOnlyPublicKey, Client, Event, EventBuilder, EventId, Filter, Keys, Kind, Options,
    RelayPoolNotification, Tag,
};
use satsbox::{create_web_app, lnurl::handle_receipts, now};
use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;
use url::form_urlencoded::byte_serialize;
use util::{create_test_state, create_test_state2};

mod util;

const PUBKEY: &str = "4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd20";

/// lud16 lud06 lud12
#[actix_rt::test]
async fn common() -> Result<()> {
    let mut state = create_test_state().await?;
    state.setting.lnurl.privkey = None;
    state.setting.lnurl.comment_allowed = 5;

    let pubkey = hex::decode(PUBKEY)?;
    let user = state.service.get_or_create_user(pubkey.clone()).await?;
    state
        .service
        .update_user_name(user.id, Some("admin".to_owned()))
        .await?;

    let state = web::Data::new(state);
    let app = init_service(create_web_app(state.clone())).await;
    sleep(Duration::from_millis(50)).await;

    let (val, _) = util::get(&app, "/.well-known/lnurlp/unknown").await?;
    assert_eq!(val["status"], json!("ERROR"));

    let (val, _) = util::get(&app, "/.well-known/lnurlp/admin").await?;
    assert_eq!(val["tag"], json!("payRequest"));
    assert_eq!(val["status"], json!("OK"));
    assert_eq!(val["allowsNostr"], json!(false));
    assert_eq!(
        val["commentAllowed"],
        json!(state.setting.lnurl.comment_allowed)
    );

    let metadata = val["metadata"].as_str().unwrap();
    let callback = val["callback"].as_str().unwrap();
    let (val, _) = util::get(
        &app,
        &format!(
            "{}?amount={}",
            callback,
            state.setting.lnurl.max_sendable + 1
        ),
    )
    .await?;
    assert_eq!(val["status"], json!("ERROR"));
    let (val, _) = util::get(
        &app,
        &format!(
            "{}?amount={}",
            callback,
            state.setting.lnurl.min_sendable - 1
        ),
    )
    .await?;
    assert_eq!(val["status"], json!("ERROR"));
    let (val, _) = util::get(
        &app,
        &format!(
            "{}?amount={}&comment=longtext",
            callback,
            state.setting.lnurl.min_sendable + 1
        ),
    )
    .await?;
    assert_eq!(val["status"], json!("ERROR"));

    let (val, _) = util::get(
        &app,
        &format!(
            "{}?amount={}&comment=test",
            callback,
            state.setting.lnurl.min_sendable + 1
        ),
    )
    .await?;
    assert_eq!(val["status"], json!("OK"));
    let pr = val["pr"].as_str().unwrap();
    let invoice = Invoice::from_bolt11(pr.to_owned()).unwrap();
    assert_eq!(sha256(metadata), invoice.description_hash.unwrap());
    Ok(())
}

/// lud18 payerData
#[actix_rt::test]
async fn payerdata() -> Result<()> {
    let mut state = create_test_state().await?;
    state.setting.lnurl.privkey = None;
    let pubkey = hex::decode(PUBKEY)?;
    let user = state.service.get_or_create_user(pubkey.clone()).await?;
    state
        .service
        .update_user_name(user.id, Some("admin".to_owned()))
        .await?;

    let state = web::Data::new(state);
    let app = init_service(create_web_app(state.clone())).await;
    sleep(Duration::from_millis(50)).await;

    let (val, _) = util::get(&app, "/.well-known/lnurlp/admin").await?;
    assert_eq!(val["tag"], json!("payRequest"));
    assert_eq!(val["status"], json!("OK"));

    let metadata = val["metadata"].as_str().unwrap();
    let callback = val["callback"].as_str().unwrap();
    let payerdata = serde_json::to_string(&json!({
        "name": "admin",
    }))?;

    let (val, _) = util::get(
        &app,
        &format!(
            "{}?amount={}&payerdata={}&nostr=xxxx", // ignore nostr zaps
            callback,
            state.setting.lnurl.min_sendable + 1,
            url_encode(&payerdata),
        ),
    )
    .await?;

    assert_eq!(val["status"], json!("OK"));
    let pr = val["pr"].as_str().unwrap();
    let invoice = Invoice::from_bolt11(pr.to_owned()).unwrap();
    assert_eq!(
        sha256(format!("{}{}", metadata, payerdata)),
        invoice.description_hash.unwrap()
    );
    Ok(())
}

/// nip57 zaps
#[actix_rt::test]
async fn zaps() -> Result<()> {
    let user_keys = Keys::generate();

    let mut state = create_test_state().await?;
    state.service.self_payment = true;
    let admin_keys = Keys::generate();
    state.setting.donation.privkey = Some(admin_keys.secret_key()?.into());
    state.service.donation_receiver = Some(admin_keys.public_key().serialize().to_vec());
    state.setting.lnurl.privkey = Some(admin_keys.secret_key()?.into());

    let state = web::Data::new(state);

    let server_keys = Keys::new(state.setting.lnurl.privkey.unwrap().into());

    let admin = state
        .service
        .get_or_create_user(admin_keys.public_key().serialize().to_vec())
        .await?;
    state
        .service
        .update_user_name(admin.id, Some("admin".to_owned()))
        .await?;

    let user = state
        .service
        .get_or_create_user(user_keys.public_key().serialize().to_vec())
        .await?;
    let user = state
        .service
        .admin_adjust_user_balance(&user, 1_000_000, None)
        .await?;

    let app = init_service(create_web_app(state.clone())).await;
    sleep(Duration::from_millis(50)).await;

    let (val, _) = util::get(&app, "/.well-known/lnurlp/admin").await?;
    assert_eq!(val["tag"], json!("payRequest"));
    assert_eq!(val["status"], json!("OK"));
    assert_eq!(val["allowsNostr"], json!(true));
    assert_eq!(
        val["nostrPubkey"],
        json!(server_keys.public_key().to_string())
    );
    let callback = val["callback"].as_str().unwrap();

    let amount = 10_000;
    let rel_event_id = EventId::from_slice(&user_keys.public_key().serialize()).unwrap();

    let event = create_zap_request_event(
        &user_keys,
        &state.setting.lnurl.relays,
        amount,
        user_keys.public_key(),
        Some(rel_event_id.clone()),
    )?;
    let event_json = event.as_json();
    let (val, _) = util::get(
        &app,
        &format!(
            "{}?amount={}&nostr={}",
            callback,
            amount + 1,
            url_encode(&event_json),
        ),
    )
    .await?;
    assert_eq!(val["status"], json!("ERROR"));
    assert!(val["reason"].as_str().unwrap().contains("the same amount"));

    let (val, _) = util::get(
        &app,
        &format!(
            "{}?amount={}&nostr={}",
            callback,
            amount,
            url_encode(&event_json),
        ),
    )
    .await?;

    assert_eq!(val["status"], json!("OK"));
    let pr = val["pr"].as_str().unwrap();
    let invoice = Invoice::from_bolt11(pr.to_owned()).unwrap();
    assert_eq!(sha256(&event_json), invoice.description_hash.unwrap());

    let count = handle_receipts(&state).await?;
    assert_eq!(count, 0);

    // self payment
    state
        .service
        .pay(
            &user,
            pr.to_owned(),
            &state.setting.fee,
            entity::invoice::Source::Test,
            false,
        )
        .await?;

    let user = state
        .service
        .get_or_create_user(user_keys.public_key().serialize().to_vec())
        .await?;
    assert_eq!(user.donate_amount, amount as i64);

    let count = handle_receipts(&state).await?;
    assert_eq!(count, 1);
    let count = handle_receipts(&state).await?;
    assert_eq!(count, 0);

    // check events
    let opts = Options::new();
    let client = Client::with_opts(&user_keys, opts);
    for url in &state.setting.lnurl.relays {
        client.add_relay(url.as_str(), None).await?;
    }
    client.connect().await;

    let filter = Filter::new()
        .kind(Kind::ZapReceipt)
        .pubkey(user_keys.public_key());
    client.subscribe(vec![filter]).await;

    let event = wait(&client, 5, |notification| async {
        match notification {
            RelayPoolNotification::Event(_url, event) => {
                if event.kind == Kind::ZapReceipt {
                    return Ok(Some(event));
                }
            }
            _ => {}
        }
        Ok(None)
    })
    .await?;

    // validate zap
    assert_eq!(event.kind, Kind::ZapReceipt);
    let description = event
        .tags
        .iter()
        .find_map(|t| {
            if let Tag::Description(r) = t {
                Some(r.clone())
            } else {
                None
            }
        })
        .unwrap();
    let bolt11 = event
        .tags
        .iter()
        .find_map(|t| {
            if let Tag::Bolt11(r) = t {
                Some(r.clone())
            } else {
                None
            }
        })
        .unwrap();
    let invoice = Invoice::from_bolt11(bolt11).unwrap();
    assert_eq!(sha256(&description), invoice.description_hash.unwrap());

    Ok(())
}

fn url_encode(s: &str) -> String {
    byte_serialize(s.as_bytes()).collect::<String>()
}

fn create_zap_request_event(
    client_keys: &Keys,
    relays: &[String],
    amount: u64,
    p: XOnlyPublicKey,
    e: Option<EventId>,
) -> Result<Event> {
    let relays = relays.iter().map(|s| s.into()).collect();
    let mut tags = vec![
        Tag::PubKey(p, None),
        Tag::Relays(relays),
        Tag::Amount(amount),
    ];
    if let Some(e) = e {
        tags.push(Tag::Event(e, None, None));
    }
    Ok(EventBuilder::new(Kind::ZapRequest, "", &tags).to_event(client_keys)?)
}

async fn wait<F, Fut, O>(client: &Client, timeout_seconds: u64, func: F) -> satsbox::Result<O>
where
    F: Fn(RelayPoolNotification) -> Fut,
    Fut: std::future::Future<Output = satsbox::Result<Option<O>>>,
{
    timeout(Duration::from_secs(timeout_seconds), _wait(client, func))
        .await
        .map_err(|_| satsbox::Error::Str("timeout"))?
}

async fn _wait<F, Fut, O>(client: &Client, func: F) -> satsbox::Result<O>
where
    F: Fn(RelayPoolNotification) -> Fut,
    Fut: std::future::Future<Output = satsbox::Result<Option<O>>>,
{
    let mut notifications = client.notifications();
    while let Ok(notification) = notifications.recv().await {
        let r = func(notification)
            .await
            .map_err(|e| satsbox::Error::Message(e.to_string()))?;
        if r.is_some() {
            return Ok(r.unwrap());
        }
    }
    Err(satsbox::Error::Str("?"))
}

#[actix_rt::test]
async fn donate_by_lud18() -> Result<()> {
    let payer_state = create_test_state2(Some(satsbox::setting::Lightning::Cln)).await?;
    let mut state = create_test_state2(Some(satsbox::setting::Lightning::Lnd)).await?;
    let donation_keys = Keys::generate();
    state.setting.donation.privkey = Some(donation_keys.secret_key()?.into());
    state.service.donation_receiver = Some(donation_keys.public_key().serialize().to_vec());

    let donation_receiver_pubkey = donation_keys.public_key().serialize().to_vec();
    let donation_receiver = state
        .service
        .get_or_create_user(donation_receiver_pubkey.clone())
        .await?;
    state
        .service
        .update_user_name(donation_receiver.id, Some("donation".to_owned()))
        .await?;

    let payer_pubkey =
        hex::decode("4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd20")?;
    let payer = state
        .service
        .get_or_create_user(payer_pubkey.clone())
        .await?;

    let payer = state
        .service
        .admin_adjust_user_balance(&payer, 1_000_000, None)
        .await?;

    let state = web::Data::new(state);
    let app = init_service(create_web_app(state.clone())).await;
    sleep(Duration::from_millis(50)).await;

    let (val, _) = util::get(&app, "/.well-known/lnurlp/donation").await?;
    assert_eq!(val["tag"], json!("payRequest"));
    assert_eq!(val["status"], json!("OK"));

    // let metadata = val["metadata"].as_str().unwrap();
    let callback = val["callback"].as_str().unwrap();
    let donor_pubkey =
        hex::decode("4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd21")?;
    let payerdata = serde_json::to_string(&json!({
        "name": "tester",
        "pubkey": hex::encode(&donor_pubkey),
    }))?;
    let amount = 10_000;

    let (val, _) = util::get(
        &app,
        &format!(
            "{}?amount={}&payerdata={}", // ignore nostr zaps
            callback,
            amount, // 10 sats
            url_encode(&payerdata),
        ),
    )
    .await?;

    assert_eq!(val["status"], json!("OK"));
    let pr = val["pr"].as_str().unwrap();

    // internal payment
    state
        .service
        .pay(
            &payer,
            pr.to_owned(),
            &state.setting.fee,
            entity::invoice::Source::Test,
            false,
        )
        .await?;

    let donor = state
        .service
        .get_or_create_user(donor_pubkey.clone())
        .await?;

    assert_eq!(donor.donate_amount, amount);

    // external payment
    let (val, _) = util::get(
        &app,
        &format!(
            "{}?amount={}&payerdata={}", // ignore nostr zaps
            callback,
            amount, // 10 sats
            url_encode(&payerdata),
        ),
    )
    .await?;

    assert_eq!(val["status"], json!("OK"));
    let pr = val["pr"].as_str().unwrap();

    let payer_service = &payer_state.service;
    let payer_pubkey =
        hex::decode("000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca36")?;
    let payer_user = payer_service
        .get_or_create_user(payer_pubkey.clone())
        .await?;
    let balance = 5_000_000;
    let payer_user = payer_service
        .admin_adjust_user_balance(&payer_user, balance, None)
        .await?;
    payer_service
        .pay(
            &payer_user,
            pr.to_owned(),
            &state.setting.fee,
            entity::invoice::Source::Test,
            false,
        )
        .await?;
    sleep(Duration::from_secs(1)).await;
    let count = state.service.sync_invoices(now() - 60).await?;
    assert_eq!(count, 1);

    let donor = state
        .service
        .get_or_create_user(donor_pubkey.clone())
        .await?;

    assert_eq!(donor.donate_amount, amount * 2);
    Ok(())
}
