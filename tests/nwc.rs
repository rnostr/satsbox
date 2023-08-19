use actix_rt::time::sleep;
use anyhow::Result;
use nostr_sdk::{
    nips,
    secp256k1::{SecretKey, XOnlyPublicKey},
    Client, Event, EventBuilder, EventId, Filter, Keys, Kind, Options, RelayPoolNotification, Tag,
};
use satsbox::{now, nwc, AppState};
use serde_json::{json, Value};
use std::{str::FromStr, sync::Arc, time::Duration};
use tokio::time::timeout;
use util::create_test_state;

mod util;

#[actix_rt::test]
async fn get_balance() -> Result<()> {
    // if std::env::var("RUST_LOG").is_err() {
    //     std::env::set_var("RUST_LOG", "DEBUG");
    // }
    // tracing_subscriber::fmt::init();

    let mut state = create_test_state().await?;
    state.setting.nwc.privkey = Some(SecretKey::from_str(
        "6b911fd37cdf5c81d4c0adb1ab7fa822ed253ab0ad9aa18d77257c88b29b0000",
    )?);

    let server_keys = Keys::new(state.setting.nwc.privkey.unwrap());

    let client_priv =
        SecretKey::from_str("7b911fd37cdf5c81d4c0adb1ab7fa822ed253ab0ad9aa18d77257c88b29b0000")?;
    let client_keys = Keys::new(client_priv);

    let state = Arc::new(state);
    // let app = init_service(create_web_app(state.clone())).await;

    let nwc = nwc::Nwc::new(state.clone());
    nwc.connect().await?;
    let handle = tokio::spawn(async move { nwc.handle_notifications().await });

    sleep(Duration::from_millis(300)).await;

    let client = connect(client_priv, &state).await?;

    sleep(Duration::from_millis(100)).await;

    let response = Filter::new()
        .kind(Kind::WalletConnectResponse)
        .pubkey(client_keys.public_key())
        .since((now() - 60 * 5).into());

    let info = Filter::new()
        .kind(Kind::WalletConnectInfo)
        .author(server_keys.public_key().to_string());

    client.subscribe(vec![response, info]).await;

    let res = wait(&client, 5, |notification| async {
        match notification {
            RelayPoolNotification::Event(_url, event) => {
                if event.kind == Kind::WalletConnectInfo {
                    return Ok(Some(event));
                }
            }
            _ => {}
        }
        Ok(None)
    })
    .await?;
    assert_eq!(res.content, nwc::METHODS);

    let res = request(
        &client,
        &server_keys,
        &client_keys,
        "get_balance",
        json!({}),
        5,
    )
    .await?;
    assert_eq!(res["balance"], json!(0));
    // set balance
    let sats = 100;
    let user = state
        .service
        .get_or_create_user(client_keys.public_key().serialize().to_vec())
        .await?;
    state
        .service
        .admin_adjust_user_balance(&user, sats * 1000, None)
        .await?;

    let res = request(
        &client,
        &server_keys,
        &client_keys,
        "get_balance",
        json!({}),
        5,
    )
    .await?;
    assert_eq!(res["balance"], json!(sats));

    handle.abort();
    Ok(())
}

async fn request(
    client: &Client,
    server_keys: &Keys,
    client_keys: &Keys,
    method: &str,
    params: Value,
    timeout_seconds: u64,
) -> anyhow::Result<Value> {
    let event = create_request_event(server_keys.public_key(), client_keys, method, params)?;
    let event_id = event.id.clone();
    client.send_event(event).await?;
    let res = wait(&client, timeout_seconds, |notification| async {
        match notification {
            RelayPoolNotification::Event(_url, event) => {
                if event.kind == Kind::WalletConnectResponse {
                    let res = parse_response(&event, &client_keys, &event_id)?;
                    return Ok(Some(res));
                }
            }
            _ => {}
        }
        Ok(None)
    })
    .await?;
    if res.0 != method {
        return Err(anyhow::Error::new(satsbox::Error::Str("method not match")));
    }
    Ok(res.1)
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

async fn connect(privkey: SecretKey, state: &AppState) -> Result<Client> {
    let keys = Keys::new(privkey);
    let opts = Options::new();
    let client = Client::with_opts(&keys, opts);

    for url in &state.setting.nwc.relays {
        client.add_relay(url, None).await?;
    }
    client.connect().await;
    Ok(client)
}

fn create_request_event(
    server_pubkey: XOnlyPublicKey,
    client_keys: &Keys,
    method: &str,
    params: Value,
) -> Result<Event> {
    let content = json!({
        "method": method,
        "params": params,
    });
    let content = nips::nip04::encrypt(
        &client_keys.secret_key().unwrap(),
        &server_pubkey,
        serde_json::to_string(&content)?,
    )?;
    Ok(EventBuilder::new(
        23194.into(),
        content,
        &vec![Tag::PubKey(server_pubkey, None)],
    )
    .to_event(client_keys)?)
}

fn parse_response(
    event: &Event,
    keys: &Keys,
    request_event_id: &EventId,
) -> satsbox::Result<(String, Value)> {
    if event.kind != Kind::WalletConnectResponse
        || !event.tags.iter().any(|t| match t {
            Tag::PubKey(k, _) => k == &keys.public_key(),
            _ => false,
        })
        || !event.tags.iter().any(|t| match t {
            Tag::Event(k, _, _) => k == request_event_id,
            _ => false,
        })
    {
        return Err(satsbox::Error::Str("Invalid event kind or tags"));
    }

    let content = nips::nip04::decrypt(
        &keys.secret_key().unwrap(),
        &event.pubkey,
        event.content.clone(),
    )?;
    let val: Value = serde_json::from_str(&content)?;
    if !val["result_type"].is_string() {
        return Err(satsbox::Error::Str("Invalid method"));
    }

    if val["error"]["message"].is_string() {
        return Err(satsbox::Error::Message(
            val["error"]["message"].as_str().unwrap().to_owned(),
        ));
    }

    Ok((
        val["result_type"].as_str().unwrap().to_owned(),
        val["result"].clone(),
    ))
}
