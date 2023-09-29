//! http api

use crate::{auth, full_uri_from_req, key::Privkey, setting::Setting, AppState, Error, Result};
use actix_web::{get, http::Uri, post, web, HttpRequest, HttpResponse, Responder, Scope};
use entity::user;
use nostr_sdk::{prelude::ToBech32, secp256k1::XOnlyPublicKey, Keys};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub const CARGO_PKG_VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

fn version() -> String {
    CARGO_PKG_VERSION.map(ToOwned::to_owned).unwrap_or_default()
}

pub fn scope() -> Scope {
    web::scope("/v1")
        .service(info)
        .service(post_auth)
        .service(get_auth)
        .service(my)
        .service(reset_lndhub)
        .service(update_username)
        .service(pay_invoice)
}

fn privkey_to_pubkey(k: Privkey) -> String {
    Keys::new(k.into()).public_key().to_string()
}

#[get("/info")]
pub async fn info(state: web::Data<AppState>, req: HttpRequest) -> Result<HttpResponse, Error> {
    let uri = full_uri_from_req(&req);

    let info = state.service.info().await?;

    let username_chars: Vec<usize> = (0..state.setting.donation.amounts.len())
        .map(|i| i + 2)
        .rev()
        .collect();

    let host = uri.authority().map(|a| a.as_str()).unwrap_or_default();
    let donation_address = state.setting.donation.privkey.map(|k| {
        format!(
            "{}@{}",
            Keys::new(k.into()).public_key().to_bech32().unwrap(),
            host,
        )
    });

    Ok(HttpResponse::Ok().json(json!({
        "version": version(),
        "node": {
            "id": hex::encode(info.id),
            "version": info.version,
        },
        "fee": state.setting.fee,
        "donation": {
            "pubkey": state.setting.donation.privkey.map(privkey_to_pubkey),
            "address": donation_address,
            "amounts": state.setting.donation.amounts,
            "restrict_username": state.setting.donation.restrict_username,
            "username_chars": username_chars,
        },
        "nwc": {
            "pubkey": state.setting.nwc.privkey.map(privkey_to_pubkey),
            "relays": state.setting.nwc.relays,
        },
    })))
}

#[post("/auth")]
pub async fn post_auth(
    _state: web::Data<AppState>,
    _data: auth::Json<Value>,
) -> Result<impl Responder, Error> {
    Ok(web::Json(json!({"success": true})))
}

#[get("/auth")]
pub async fn get_auth(
    _state: web::Data<AppState>,
    _user: auth::NostrAuth,
) -> Result<impl Responder, Error> {
    Ok(web::Json(json!({"success": true})))
}

const USERNAME_MAX_CHARS: usize = 20;

fn get_username_setting(setting: &Setting, donate_amount: u64) -> (bool, usize) {
    if setting.donation.restrict_username {
        let level = setting.donation.level(donate_amount);
        if let Some(level) = level {
            let total = setting.donation.amounts.len();
            (true, total - level + 1)
        } else {
            (false, 2)
        }
    } else {
        (true, 2)
    }
}

/// current user info
#[get("/my")]
pub async fn my(
    state: web::Data<AppState>,
    nostr_user: auth::NostrAuth,
) -> Result<impl Responder, Error> {
    // don't create account
    let user = state
        .service
        .get_user(nostr_user.pubkey.clone())
        .await?
        .unwrap_or_default();

    let pubkey = hex::encode(nostr_user.pubkey.clone());
    let host = nostr_user
        .url
        .authority()
        .map(|a| a.as_str())
        .unwrap_or_default();
    let address = format!(
        "{}@{}",
        user.username.clone().unwrap_or_else(|| {
            XOnlyPublicKey::from_slice(&nostr_user.pubkey)
                .unwrap()
                .to_bech32()
                .unwrap()
        }),
        host,
    );

    let (allowed, min) = get_username_setting(&state.setting, user.donate_amount as u64);
    Ok(web::Json(json!({"user": {
        "pubkey": pubkey,
        "address": address,
        "balance": user.balance,
        "lock_amount": user.lock_amount,
        "username": user.username,
        "donate_amount": user.donate_amount,
        "lndhub": lndhub_info(&nostr_user.url, &user),
        "allow_update_username": allowed,
        "allow_update_username_min_chars": min,
        "allow_update_username_max_chars": USERNAME_MAX_CHARS,
    }})))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ResetLndhubReq {
    disable: bool,
}

fn lndhub_info(uri: &Uri, user: &user::Model) -> Value {
    let pubkey = hex::encode(&user.pubkey);
    let url = user.password.clone().map(|p| {
        format!(
            "lndhub://{}:{}@{}://{}",
            pubkey.clone(),
            p,
            uri.scheme_str().unwrap_or_default(),
            uri.authority().map(|a| a.as_str()).unwrap_or_default()
        )
    });
    json!({
        "login": pubkey,
        "password": user.password,
        "url": url,
    })
}

/// reset lndhub password
#[post("/reset_lndhub")]
pub async fn reset_lndhub(
    state: web::Data<AppState>,
    nostr_user: auth::NostrAuth,
    // data had handled by nostr auth
    // data: web::Json<ResetLndhubReq>,
    // req: HttpRequest,
) -> Result<impl Responder, Error> {
    let data: ResetLndhubReq = serde_json::from_slice(&nostr_user.payload)?;
    let user = state
        .service
        .get_or_create_user(nostr_user.pubkey.clone())
        .await?;
    let password = if data.disable {
        None
    } else {
        Some(rand_password())
    };

    let user = state
        .service
        .update_user_password(user.id, password)
        .await?;

    Ok(web::Json(json!({
        "lndhub": lndhub_info(&nostr_user.url, &user)
    })))
}

fn rand_password() -> String {
    let mut store_key_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut store_key_bytes);
    hex::encode(store_key_bytes)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UpdateUsernameReq {
    username: Option<String>,
}

/// reset lndhub password
#[post("/update_username")]
pub async fn update_username(
    state: web::Data<AppState>,
    nostr_user: auth::NostrAuth,
    // data: web::Json<UpdateUsernameReq>,
) -> Result<impl Responder, Error> {
    let data: UpdateUsernameReq = serde_json::from_slice(&nostr_user.payload)?;

    let user = state
        .service
        .get_or_create_user(nostr_user.pubkey.clone())
        .await?;

    let (allowed, min) = get_username_setting(&state.setting, user.donate_amount as u64);
    if !allowed {
        return Err(Error::InvalidParam(
            "Username changes are not allowed".to_string(),
        ));
    }
    if let Some(username) = &data.username {
        let len = username.len();
        if len > USERNAME_MAX_CHARS {
            return Err(Error::InvalidParam(format!(
                "The length of the username cannot be greater than {}",
                USERNAME_MAX_CHARS
            )));
        }
        if len < min {
            return Err(Error::InvalidParam(format!(
                "The length of the username cannot be less than {}",
                min
            )));
        }
        // a-z0-9-_.
        if !username
            .chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-' | '_' | '.'))
        {
            return Err(Error::InvalidParam(
                "The username can only contain the characters a-z 0-9 - _ .".to_string(),
            ));
        }
    }

    state
        .service
        .update_username(user.id, data.username.clone())
        .await?;

    Ok(web::Json(json!({"success": true})))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PayInvoiceReq {
    invoice: String,
}

/// pay invoice api
#[post("/pay_invoice")]
pub async fn pay_invoice(
    state: web::Data<AppState>,
    nostr_user: auth::NostrAuth,
) -> Result<impl Responder, Error> {
    let data: PayInvoiceReq = serde_json::from_slice(&nostr_user.payload)?;
    let user = state.service.get_user(nostr_user.pubkey.clone()).await?;

    if let Some(user) = user {
        let payment = state
            .service
            .pay(
                &user,
                data.invoice,
                &state.setting.fee,
                entity::invoice::Source::Api,
                false,
            )
            .await?;
        Ok(web::Json(json!({
            "preimage": hex::encode(payment.payment_preimage)
        })))
    } else {
        Err(Error::InsufficientBalance)
    }
}
