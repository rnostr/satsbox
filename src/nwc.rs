//! nostr wallet connect api

use crate::{now, AppState, Error, Result};
use futures::FutureExt;
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use nostr_sdk::{
    prelude::{
        nips, Client, Event, EventBuilder, Filter, Keys, Kind, Options, RelayPoolNotification, Tag,
        XOnlyPublicKey,
    },
    EventId,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{net::SocketAddr, str::FromStr, sync::Arc};

pub const METHODS: &str = "pay_invoice get_balance";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestMethod {
    PayInvoice,
    GetBalance,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct PayInvoiceParam {
    pub invoice: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: RequestMethod,
    #[serde(default)]
    pub params: Value,
}

fn parse_request(event: &Event, keys: &Keys) -> Result<Request> {
    if event.kind != Kind::WalletConnectRequest
        || !event.tags.iter().any(|t| match t {
            Tag::PubKey(k, _) => k == &keys.public_key(),
            _ => false,
        })
    {
        return Err(Error::Str("Invalid event kind or tags"));
    }

    let content = nips::nip04::decrypt(
        &keys.secret_key().unwrap(),
        &event.pubkey,
        event.content.clone(),
    )?;

    Ok(serde_json::from_str(&content)?)
}

async fn handle_request(pubkey: Vec<u8>, req: Request, nwc: &Nwc) -> Result<Value> {
    nwc.limiter_per_second
        .check()
        .map_err(|_| Error::RateLimited)?;

    let state = &nwc.state;

    match req.method {
        RequestMethod::PayInvoice => {
            let params: PayInvoiceParam = serde_json::from_value(req.params)?;
            let user = state.service.get_user(pubkey).await?;
            match user {
                Some(user) => {
                    let payment = state
                        .service
                        .pay(
                            &user,
                            params.invoice,
                            &state.setting.fee,
                            "nwc".to_string(),
                            false,
                        )
                        .await?;
                    let image = hex::encode(payment.payment_preimage);
                    Ok(json!({
                        "preimage": image,
                    }))
                }
                None => Err(Error::InsufficientBalance),
            }
        }
        RequestMethod::GetBalance => {
            let user = state.service.get_user(pubkey).await?;

            let sats = user
                .map(|u: ::entity::user::Model| u.balance - u.lock_amount)
                .unwrap_or_default()
                / 1000;
            Ok(json!({
                "balance": sats,
            }))
        }
    }
}

// nip47 respose event kind: 23195
fn create_response_event(
    user_pubkey: XOnlyPublicKey,
    request_event_id: EventId,
    keys: &Keys,
    content: Value,
) -> Result<Event> {
    let content = nips::nip04::encrypt(
        &keys.secret_key().unwrap(),
        &user_pubkey,
        serde_json::to_string(&content)?,
    )?;
    Ok(EventBuilder::new(
        Kind::WalletConnectResponse,
        content,
        &vec![
            Tag::PubKey(user_pubkey, None),
            Tag::Event(request_event_id, None, None),
        ],
    )
    .to_event(keys)?)
}

// nip47 info event kind: 13194
fn create_info_event(keys: &Keys) -> Result<Event> {
    Ok(EventBuilder::new(Kind::WalletConnectInfo, METHODS, &[]).to_event(keys)?)
}

fn error_response(err: &Error, method: RequestMethod) -> Value {
    let code = match err {
        Error::InsufficientBalance => "INSUFFICIENT_BALANCE",
        Error::RateLimited => "RATE_LIMITED",
        _ => "INTERNAL",
    };
    json!({
        "result_type": method,
        "error": {
            "code": code,
            "message": err.to_string(),
        }
    })
}

async fn handle_event(event: Event, nwc: Nwc) -> Result<()> {
    // TODO: check repeat event from db, log event
    let req = parse_request(&event, &nwc.keys)?;
    let method = req.method.clone();
    let user_pubkey = event.pubkey;

    let res = handle_request(user_pubkey.serialize().to_vec(), req, &nwc).await;
    let res = match res {
        Ok(res) => {
            json!({
                "result_type": method,
                "result": res,
            })
        }
        Err(err) => error_response(&err, method),
    };

    nwc.client
        .send_event(create_response_event(
            user_pubkey,
            event.id,
            &nwc.keys,
            res,
        )?)
        .await?;

    Ok(())
}

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

#[derive(Clone)]
pub struct Nwc {
    client: Arc<Client>,
    state: Arc<AppState>,
    limiter_per_second: Arc<Limiter>,
    keys: Keys,
}

impl Nwc {
    pub fn new(state: Arc<AppState>) -> Self {
        let lim = RateLimiter::direct(Quota::per_second(state.setting.nwc.rate_limit_per_second));
        let keys = Keys::new(state.setting.nwc.privkey.unwrap());
        let opts = Options::new();
        let client = Client::with_opts(&keys, opts);
        Self {
            client: Arc::new(client),
            state,
            limiter_per_second: Arc::new(lim),
            keys,
        }
    }

    pub async fn connect(&self) -> Result<()> {
        let proxy = if let Some(proxy) = &self.state.setting.nwc.proxy {
            Some(SocketAddr::from_str(proxy)?)
        } else {
            None
        };

        for url in &self.state.setting.nwc.relays {
            self.client.add_relay(url, proxy).await?;
        }
        self.client.connect().await;

        self.client
            .send_event(create_info_event(&self.keys)?)
            .await?;

        let subscription = Filter::new()
            .kind(Kind::WalletConnectRequest)
            .pubkey(self.keys.public_key())
            .since((now() - 60 * 5).into()); // since last 5 minutes

        self.client.subscribe(vec![subscription]).await;
        Ok(())
    }

    pub async fn handle_notifications(&self) -> Result<()> {
        self.client
            .handle_notifications(|notification| async {
                tracing::debug!("message: {:?}", notification);
                // had de-duplicate
                if let RelayPoolNotification::Event(url, event) = notification {
                    // run synchronous
                    let _r =
                        tokio::spawn(handle_event(event.clone(), self.clone())).then(|res| async {
                            if let Ok(Err(err)) = &res {
                                tracing::error!(
                                    "handle event error: {:?} {:?} {:?}",
                                    err,
                                    url,
                                    event
                                );
                            }
                            res
                        });
                }
                Ok(())
            })
            .await?;
        Ok(())
    }
}
