//! lnurl api

use crate::{full_uri_from_req, AppState, Error, InvoiceExtra, Result};
use actix_web::{
    get, http::StatusCode, http::Uri, web, HttpRequest, HttpResponse, Responder, ResponseError,
    Scope,
};
use entity::invoice;
use nostr_sdk::{
    prelude::FromBech32, secp256k1::XOnlyPublicKey, Client, Event, EventId, Keys, Kind, Options,
    Tag, Timestamp, UnsignedEvent,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, FromQueryResult, QueryFilter, QuerySelect,
    Set,
};
use serde::Deserialize;
use serde_aux::prelude::deserialize_number_from_string;
use serde_json::json;
use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};
use tokio::time::sleep;

#[derive(thiserror::Error, Debug)]
pub enum LnurlError {
    #[error(transparent)]
    Base(#[from] Error),
    #[error("{0}")]
    Invalid(String),
}

impl ResponseError for LnurlError {
    fn status_code(&self) -> StatusCode {
        StatusCode::OK
    }
    /// Creates full response for error.
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(json!({
            "status": "ERROR",
            "reason": self.to_string()
        }))
    }
}

pub fn scope() -> Scope {
    web::scope("/lnurlp").service(info).service(create_invoice)
}

fn metadata(host: &str, username: &String) -> Result<String> {
    let id = format!("{}@{}", username, host);
    let metadata = json!([
        [
            "text/plain", // mandatory,
            "Sats for "
        ],
        ["text/identifier", id], // lud16 mandatory
    ]);
    Ok(serde_json::to_string(&metadata)?)
}

fn host_from_uri(uri: &Uri) -> &str {
    uri.authority().map(|a| a.as_str()).unwrap_or("")
}

// LUD-06 lnurlp/{usename}
// LUD-16 .well-known/lnurlp/{usename}
// usename: bech32-serialized pubkey or a-z0-9-_. username
// LUD-18 payerData
// LUD-12 commentAllowed

#[get("/{usename}")]
pub async fn info(
    req: HttpRequest,
    state: web::Data<AppState>,
    username: web::Path<String>,
) -> Result<impl Responder, LnurlError> {
    let username = username.into_inner();
    // check username
    let pubkey = match XOnlyPublicKey::from_bech32(&username) {
        Ok(key) => key.serialize().to_vec(),
        Err(_) => {
            let user = state
                .service
                .get_user_by_name(username.clone())
                .await?
                .ok_or(Error::Str("invalid user"))?;
            user.pubkey
        }
    };
    state.setting.auth.check_permission(&pubkey)?;

    let (allow, pubkey) = if let Some(key) = state.setting.lnurl.privkey {
        let keys = Keys::new(key.into());
        (true, keys.public_key().to_string())
    } else {
        (false, Default::default())
    };
    let uri = full_uri_from_req(&req);

    let metadata = metadata(host_from_uri(&uri), &username)?;
    Ok(web::Json(json!({
        "tag": "payRequest",
        "status": "OK",
        "metadata": metadata,
        "commentAllowed": state.setting.lnurl.comment_allowed,
        "maxSendable": state.setting.lnurl.max_sendable,
        "minSendable": state.setting.lnurl.min_sendable,
        "callback": format!("{}/callback", uri),
        "allowsNostr": allow,
        "nostrPubkey": pubkey,
        "payerData": {
            "name": {
                "mandatory": false
            },
            "email": {
                "mandatory": false
            },
            "pubkey": {
                "mandatory": false
            }
        },
    })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct InvoiceReq {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub amount: u64,
    pub nostr: Option<String>,
    pub comment: Option<String>,
    pub payerdata: Option<String>,
}

#[get("/{usename}/callback")]
pub async fn create_invoice(
    req: HttpRequest,
    state: web::Data<AppState>,
    username: web::Path<String>,
    query: web::Query<InvoiceReq>,
) -> Result<impl Responder, LnurlError> {
    let uri = full_uri_from_req(&req);

    let username = username.into_inner();
    let setting = &state.setting.lnurl;
    let amount = query.amount;
    if amount < setting.min_sendable || amount > setting.max_sendable {
        return Err(LnurlError::Invalid(format!(
            "Amount out of bounds (min: {} sat, max: {} sat).",
            state.setting.lnurl.min_sendable / 1000,
            state.setting.lnurl.max_sendable / 1000,
        )));
    }

    let payer_data = query.payerdata.clone();

    let comment = query.comment.clone();
    if let Some(comment) = &comment {
        if comment.len() > setting.comment_allowed {
            return Err(LnurlError::Invalid(format!(
                "Comment too long (max: {} characters).",
                setting.comment_allowed
            )));
        }
    }

    let event_str = query.nostr.clone().unwrap_or_default();
    let (memo, extra) = if state.setting.lnurl.privkey.is_some() && !event_str.is_empty() {
        let event = Event::from_json(&event_str).map_err(Error::from)?;
        // https://github.com/nostr-protocol/nips/blob/master/57.md#appendix-d-lnurl-server-zap-request-validation
        if event.kind != Kind::ZapRequest {
            return Err(LnurlError::Invalid(format!(
                "Nostr event kind must be {}.",
                Kind::ZapRequest.as_u32()
            )));
        }

        let mut relays = vec![];
        let mut e_count = 0;
        let mut p_count = 0;
        let mut amount = None;
        for tag in &event.tags {
            match tag {
                Tag::Relays(r) => {
                    relays = r.clone();
                }
                Tag::PubKey(_, _) => {
                    p_count += 1;
                }
                Tag::Event(_, _, _) => {
                    e_count += 1;
                }
                Tag::Amount(num) => amount = Some(*num),
                _ => {}
            }
        }

        if p_count != 1 {
            return Err(LnurlError::Invalid(
                "Nostr event must have exactly one pubkey tag".to_owned(),
            ));
        }

        if e_count > 1 {
            return Err(LnurlError::Invalid(
                "Nostr event must have have 0 or 1 event tags".to_owned(),
            ));
        }
        if relays.is_empty() {
            return Err(LnurlError::Invalid(
                "Nostr event must have at least one relay".to_owned(),
            ));
        }
        if let Some(num) = amount {
            if num != query.amount {
                return Err(LnurlError::Invalid(
                    "Nostr event must have the same amount".to_owned(),
                ));
            }
        }
        let extra = InvoiceExtra {
            source: "zap".to_owned(),
            zap: true,
            comment,
            zap_receipt: None,
            payer_data,
        };
        (event_str, extra)
    } else {
        let extra = InvoiceExtra {
            source: "lnurlp".to_owned(),
            zap: false,
            comment,
            zap_receipt: None,
            payer_data,
        };
        // lud06, lud18 description hash
        let memo = format!(
            "{}{}",
            metadata(host_from_uri(&uri), &username,)?,
            query.payerdata.clone().unwrap_or_default(),
        );
        (memo, extra)
    };

    let user = if let Ok(pubkey) = XOnlyPublicKey::from_bech32(&username) {
        let pubkey = pubkey.serialize().to_vec();
        state.setting.auth.check_permission(&pubkey)?;
        state.service.get_or_create_user(pubkey).await?
    } else {
        let user = state
            .service
            .get_user_by_name(username)
            .await?
            .ok_or(Error::Str("invalid user"))?;
        state.setting.auth.check_permission(&user.pubkey)?;
        user
    };

    let expiry = 3600 * 24; // one day

    let invoice = state
        .service
        .create_invoice(&user, memo, amount, expiry, extra)
        .await?;
    let routes: Vec<String> = vec![];

    // lud-09 successAction
    Ok(web::Json(json!({
        "status": "OK",
        "routes": routes,
        "pr": invoice.bolt11,
        "successAction": {
            "tag": "message",
            "message": "Thank you for your sats!"
        }
    })))
}

pub async fn loop_handle_receipts(state: Arc<AppState>, duration: Duration) -> Result<()> {
    loop {
        // TODO: log error
        let _r = handle_receipts(&state).await;
        sleep(duration).await;
    }
    // Ok(())
}

#[derive(FromQueryResult, Debug)]
struct PartInvoice {
    id: i32,
    bolt11: String,
    description: String,
    paid_at: i64,
    payment_preimage: Vec<u8>,
}

pub async fn handle_receipts(state: &AppState) -> Result<usize> {
    let keys = Keys::new(state.setting.lnurl.privkey.unwrap().into());
    let relays = &state.setting.lnurl.relays;
    let proxy = state.setting.lnurl.proxy.as_ref();

    let list = invoice::Entity::find()
        .select_only()
        .columns([
            invoice::Column::Id,
            invoice::Column::Bolt11,
            invoice::Column::Zap,
            invoice::Column::Status,
            invoice::Column::ZapStatus,
            invoice::Column::Description,
            invoice::Column::PaidAt,
            invoice::Column::PaymentPreimage,
        ])
        .filter(invoice::Column::Zap.eq(true))
        .filter(invoice::Column::ZapStatus.eq(0))
        .filter(invoice::Column::Status.eq(invoice::Status::Paid))
        .filter(invoice::Column::Type.eq(invoice::Type::Invoice))
        .into_model::<PartInvoice>()
        .all(state.service.db())
        .await?;
    let mut success = 0;
    for invoice in &list {
        // TODO: log error
        let r = send_receipt(state.service.db(), invoice, &keys, relays, proxy).await;
        if r.is_ok() {
            success += 1;
        }
    }
    Ok(success)
}

async fn send_receipt(
    db: &DbConn,
    invoice: &PartInvoice,
    keys: &Keys,
    relays: &[String],
    proxy: Option<&String>,
) -> Result<()> {
    let pubkey = keys.public_key();

    let event = Event::from_json(&invoice.description)?;
    let etag = event.tags.iter().find(|t| matches!(t, Tag::Event(_, _, _)));
    let ptag = event.tags.iter().find(|t| matches!(t, Tag::PubKey(_, _)));

    // merge extra relays and user relays
    let mut relays = relays.to_vec();
    relays.extend(
        event
            .tags
            .iter()
            .find_map(|t| {
                if let Tag::Relays(r) = t {
                    Some(r.iter().map(|r| r.to_string()).collect::<Vec<_>>())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
    );
    relays.sort();
    relays.dedup();

    let mut tags = vec![
        Tag::Bolt11(invoice.bolt11.clone()),
        Tag::Description(invoice.description.clone()),
        Tag::Preimage(hex::encode(&invoice.payment_preimage)),
    ];
    if let Some(t) = etag {
        tags.push(t.clone());
    }
    if let Some(t) = ptag {
        tags.push(t.clone());
    }

    let kind = Kind::Zap;
    let content = "".to_owned();

    let created_at: Timestamp = Timestamp::from(invoice.paid_at as u64);
    let id = EventId::new(&pubkey, created_at, &kind, &tags, &content);
    let unsigned_event = UnsignedEvent {
        id,
        pubkey,
        created_at,
        kind,
        tags,
        content,
    };
    let event = unsigned_event.sign(keys)?;
    let event_json = event.as_json();
    send_event(keys, relays, event, proxy).await?;

    // mark success
    invoice::ActiveModel {
        id: Set(invoice.id),
        zap_receipt: Set(Some(event_json)),
        zap_status: Set(1),
        ..Default::default()
    }
    .update(db)
    .await?;

    Ok(())
}

async fn send_event(
    keys: &Keys,
    relays: Vec<String>,
    event: Event,
    proxy: Option<&String>,
) -> Result<()> {
    let opts = Options::new();
    let client = Client::with_opts(keys, opts);

    let proxy = if let Some(proxy) = proxy {
        Some(SocketAddr::from_str(proxy)?)
    } else {
        None
    };

    for url in relays {
        client.add_relay(url, proxy).await?;
    }
    client.connect().await;

    client.send_event(event).await?;
    Ok(())
}
