//! lnurl api

use crate::{AppState, Error, Result};
use actix_web::{
    get, http::StatusCode, web, HttpRequest, HttpResponse, Responder, ResponseError, Scope,
};
use nostr_sdk::{prelude::FromBech32, secp256k1::XOnlyPublicKey, Keys};
use serde::Deserialize;
use serde_aux::prelude::deserialize_number_from_string;
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum LnurlError {
    #[error(transparent)]
    Base(#[from] Error),
    #[error("amount out of range")]
    AmountOutOfRange,
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

// lud06 lnurlp/{usename}
// lud16 .well-known/lnurlp/{usename}
// usename: bech32-serialized pubkey or a-z0-9-_. username

#[get("/{usename}")]
pub async fn info(
    req: HttpRequest,
    state: web::Data<AppState>,
    _username: web::Path<String>,
) -> Result<impl Responder, LnurlError> {
    // req.uri()
    // let username = username.into_inner();
    let keys = Keys::new(state.setting.nwc.privkey);
    // TODO: setting max min
    Ok(web::Json(json!({
        "tag": "payRequest",
        "metadata": "",
        "maxSendable": state.setting.lnurl.max_sendable,
        "minSendable": state.setting.lnurl.min_sendable,
        "callback": format!("{}/callback", req.uri()),
        "allowsNostr": true,
        "nostrPubkey": keys.public_key().to_string(),
    })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct InvoiceReq {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub amount: u64,
    pub nostr: Option<String>,
}

#[get("/{usename}/callback")]
pub async fn create_invoice(
    state: web::Data<AppState>,
    username: web::Path<String>,
    query: web::Query<InvoiceReq>,
) -> Result<impl Responder, LnurlError> {
    let username = username.into_inner();
    let amount = query.amount;
    if amount < state.setting.lnurl.min_sendable || amount > state.setting.lnurl.max_sendable {
        return Err(LnurlError::AmountOutOfRange);
    }

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
    let source = "lnurl".to_owned();
    let memo = "".to_owned();

    let invoice = state
        .service
        .create_invoice(&user, memo, amount, expiry, source)
        .await?;
    let routes: Vec<String> = vec![];

    Ok(web::Json(json!({
        "status": "OK",
        "routes": routes,
        "pr": invoice.bolt11,
    })))
}
