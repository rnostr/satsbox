//! lnurl api

use crate::{full_uri_from_req, AppState, Error, InvoiceExtra, Result};
use actix_web::{
    get, http::StatusCode, http::Uri, web, HttpRequest, HttpResponse, Responder, ResponseError,
    Scope,
};
use nostr_sdk::{prelude::FromBech32, secp256k1::XOnlyPublicKey, Event, Keys, Kind, Tag};
use serde::Deserialize;
use serde_aux::prelude::deserialize_number_from_string;
use serde_json::json;

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

// lud06 lnurlp/{usename}
// lud16 .well-known/lnurlp/{usename}
// usename: bech32-serialized pubkey or a-z0-9-_. username
// LUD-18 payerData

#[get("/{usename}")]
pub async fn info(
    req: HttpRequest,
    state: web::Data<AppState>,
    username: web::Path<String>,
) -> Result<impl Responder, LnurlError> {
    // let username = username.into_inner();
    let keys = Keys::new(state.setting.nwc.privkey);
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
        "allowsNostr": true,
        "nostrPubkey": keys.public_key().to_string(),
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
    let comment = query.comment.clone().unwrap_or_default();
    if comment.len() > setting.comment_allowed {
        return Err(LnurlError::Invalid(format!(
            "Comment too long (max: {} characters).",
            setting.comment_allowed
        )));
    }
    let event_str = query.nostr.clone().unwrap_or_default();
    let (memo, extra) = if event_str.is_empty() {
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
            comment: Some(comment),
            zap: true,
            zap_receipt: None,
        };
        (event_str, extra)
    } else {
        let extra = InvoiceExtra {
            source: "lnurlp".to_owned(),
            comment: Some(comment),
            ..Default::default()
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
        // pubkey
        state
            .service
            .get_or_create_user(pubkey.serialize().to_vec())
            .await?
    } else {
        state
            .service
            .get_user_by_name(username)
            .await?
            .ok_or(Error::Str("invalid user"))?
    };

    let expiry = 3600 * 24; // one day

    let invoice = state
        .service
        .create_invoice(&user, memo, amount, expiry, extra)
        .await?;
    let routes: Vec<String> = vec![];

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
