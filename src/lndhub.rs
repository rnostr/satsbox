//! lnd hub api

use crate::{
    auth::{AuthError, AuthedUser, JwtToken},
    AppState, Error, InvoiceExtra, Result,
};
use actix_web::{
    dev::Payload, get, http::StatusCode, post, web, FromRequest, HttpRequest, HttpResponse,
    Responder, ResponseError,
};
use entity::{invoice, user};
use lightning_client::lightning;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use serde_aux::prelude::deserialize_number_from_string;
use serde_json::json;
use std::{future::Future, pin::Pin};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_info)
        .service(auth)
        .service(add_invoice)
        .service(balance)
        .service(get_user_invoices)
        .service(get_btc)
        .service(get_pending)
        .service(pay_invoice)
        .service(get_txs)
        .service(check_payment);
}

/// Lndhub authed user.
/// Requires password already set.
#[derive(Debug)]
pub struct LndhubAuthedUser {
    pub user: user::Model,
}

impl TryFrom<AuthedUser> for LndhubAuthedUser {
    type Error = LndhubError;
    fn try_from(user: AuthedUser) -> Result<Self, LndhubError> {
        if user.user.password.is_some() {
            Ok(LndhubAuthedUser { user: user.user })
        } else {
            Err(Error::from(AuthError::Invalid("Unauthorized")).into())
        }
    }
}

impl FromRequest for LndhubAuthedUser {
    type Error = LndhubError;
    type Future = Pin<Box<dyn Future<Output = Result<LndhubAuthedUser, LndhubError>>>>;
    fn from_request(req: &HttpRequest, pl: &mut Payload) -> Self::Future {
        let fut = AuthedUser::from_request(req, pl);
        Box::pin(async move { LndhubAuthedUser::try_from(fut.await?) })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum LndhubError {
    #[error(transparent)]
    Base(#[from] Error),
    #[error("bad auth")]
    BadAuth,
    #[error("bad arguments")]
    BadArguments,
}

impl LndhubError {
    pub fn code(&self) -> u16 {
        match self {
            LndhubError::Base(Error::Auth(_)) => 1,
            LndhubError::BadAuth => 1,
            LndhubError::BadArguments => 8,
            LndhubError::Base(_) => 6,
        }
    }
}

impl ResponseError for LndhubError {
    fn status_code(&self) -> StatusCode {
        StatusCode::OK
    }

    /// Creates full response for error.
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(json!({
            "error": true,
            "code": self.code(),
            "message": self.to_string()
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoRes {
    #[serde(with = "hex::serde")]
    pub identity_pubkey: Vec<u8>,
    pub alias: String,
    pub color: String,
    pub num_peers: u32,
    pub num_pending_channels: u32,
    pub num_active_channels: u32,
    pub num_inactive_channels: u32,
    pub version: String,
    pub block_height: u32,
    pub uris: Vec<String>,
}

impl From<lightning::Info> for InfoRes {
    fn from(value: lightning::Info) -> Self {
        Self {
            identity_pubkey: value.id,
            alias: value.alias,
            color: value.color,
            num_peers: value.num_peers,
            num_pending_channels: value.num_pending_channels,
            num_active_channels: value.num_active_channels,
            num_inactive_channels: value.num_inactive_channels,
            version: value.version,
            block_height: value.block_height,
            uris: vec![],
        }
    }
}

#[get("/getinfo")]
pub async fn get_info(
    state: web::Data<AppState>,
    _user: LndhubAuthedUser,
) -> Result<impl Responder, LndhubError> {
    let info = state.service.info().await?;
    let mut info = InfoRes::from(info);
    info.uris.push(format!(
        "{}@{}",
        hex::encode(&info.identity_pubkey),
        state.setting.lightning_node
    ));
    Ok(web::Json(info))
    // Ok(HttpResponse::Ok().json(Info::from(info)))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AuthReq {
    login: String,
    password: String,
    refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AuthRes {
    refresh_token: String,
    access_token: String,
}

#[post("/auth")]
pub async fn auth(
    state: web::Data<AppState>,
    data: web::Json<AuthReq>,
) -> Result<HttpResponse, LndhubError> {
    let user = if !data.refresh_token.is_empty() {
        // auth from refresh token
        let user = AuthedUser::from_token(&data.refresh_token, &state).await?;
        LndhubAuthedUser::try_from(user)?.user
    } else if !data.login.is_empty() && !data.password.is_empty() {
        let user = state
            .service
            .get_user(hex::decode(&data.login).map_err(Error::from)?)
            .await?;
        if let Some(user) = user {
            if user.password.as_ref() == Some(&data.password) {
                user
            } else {
                return Err(LndhubError::BadAuth);
            }
        } else {
            return Err(LndhubError::BadAuth);
        }
    } else {
        return Err(LndhubError::BadArguments);
    };

    state.setting.auth.check_permission(&user.pubkey)?;

    let refresh_token = JwtToken::generate(
        user.id,
        state.setting.auth.refresh_token_expiry,
        state.setting.auth.secret.as_bytes(),
    )
    .map_err(Error::from)?;

    let access_token = JwtToken::generate(
        user.id,
        state.setting.auth.access_token_expiry,
        state.setting.auth.secret.as_bytes(),
    )
    .map_err(Error::from)?;

    Ok(HttpResponse::Ok().json(AuthRes {
        refresh_token,
        access_token,
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AddInvoiceReq {
    memo: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    amt: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InvoiceRes {
    payment_request: String,
    pay_req: String,
    #[serde(with = "hex::serde")]
    pub r_hash: Vec<u8>,
    #[serde(with = "hex::serde")]
    pub payment_hash: Vec<u8>,
    pub description: String,
    pub timestamp: i64,
    pub r#type: String,
    pub expire_time: i64,
    pub amt: i64,
    pub ispaid: bool,
}

impl From<invoice::Model> for InvoiceRes {
    fn from(value: invoice::Model) -> Self {
        let t = match value.r#type {
            invoice::Type::Invoice => "user_invoice".to_string(),
            invoice::Type::Payment => "paid_invoice".to_string(),
        };
        Self {
            payment_request: value.bolt11.clone(),
            pay_req: value.bolt11,
            r_hash: value.payment_hash.clone(),
            payment_hash: value.payment_hash,
            description: value.description,
            timestamp: value.generated_at,
            r#type: t,
            expire_time: value.expiry,
            amt: value.paid_amount / 1000, // real received amount
            ispaid: value.status == invoice::Status::Paid,
        }
    }
}

#[post("/addinvoice")]
pub async fn add_invoice(
    state: web::Data<AppState>,
    data: web::Json<AddInvoiceReq>,
    user: LndhubAuthedUser,
) -> Result<impl Responder, LndhubError> {
    if data.amt == 0 {
        return Err(LndhubError::BadArguments);
    }
    let expiry = 3600 * 24; // one day
    let source = invoice::Source::Lndhub;
    let invoice = state
        .service
        .create_invoice(
            &user.user,
            data.memo.clone(),
            data.amt * 1000,
            expiry,
            InvoiceExtra::new(source),
        )
        .await?;
    Ok(web::Json(InvoiceRes::from(invoice)))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PayInvoiceReq {
    invoice: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    amount: u64, // TODO: amt is used only for 'tip' invoices ?
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PayRes {
    payment_request: String,
    pay_req: String,
    #[serde(with = "hex::serde")]
    pub payment_hash: Vec<u8>,
    pub description: String,
    pub timestamp: i64,
    pub num_satoshis: i64,
    pub payment_error: String,
    #[serde(with = "hex::serde")]
    pub payment_preimage: Vec<u8>,
}

impl From<invoice::Model> for PayRes {
    fn from(value: invoice::Model) -> Self {
        Self {
            payment_request: value.bolt11.clone(),
            pay_req: value.bolt11,
            payment_hash: value.payment_hash.clone(),
            payment_preimage: value.payment_preimage,
            description: value.description,
            timestamp: value.generated_at,
            num_satoshis: value.paid_amount / 1000, // real received amount
            payment_error: "".to_string(),
        }
    }
}

#[post("/payinvoice")]
pub async fn pay_invoice(
    state: web::Data<AppState>,
    data: web::Json<PayInvoiceReq>,
    user: LndhubAuthedUser,
) -> Result<impl Responder, LndhubError> {
    let payment = state
        .service
        .pay(
            &user.user,
            data.invoice.clone(),
            &state.setting.fee,
            invoice::Source::Lndhub,
            false,
        )
        .await?;
    Ok(web::Json(PayRes::from(payment)))
}

#[get("/balance")]
pub async fn balance(
    _state: web::Data<AppState>,
    user: LndhubAuthedUser,
) -> Result<impl Responder, LndhubError> {
    // sats
    let balance = (user.user.balance - user.user.lock_amount) / 1000;
    Ok(web::Json(json!({
        "BTC": {
            "AvailableBalance": balance
        }
    })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct InvoicesReq {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub limit: u64,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub offset: u64,
}

#[get("/getuserinvoices")]
pub async fn get_user_invoices(
    state: web::Data<AppState>,
    user: LndhubAuthedUser,
    query: web::Query<InvoicesReq>,
) -> Result<impl Responder, LndhubError> {
    let mut limit = query.limit;
    if limit == 0 {
        limit = 1000;
    }
    let list = invoice::Entity::find()
        .filter(invoice::Column::UserId.eq(user.user.id))
        .filter(invoice::Column::Type.eq(invoice::Type::Invoice))
        .offset(query.offset)
        .limit(limit)
        .order_by_desc(invoice::Column::Id)
        .all(state.service.db())
        .await
        .map_err(Error::from)?;
    let list = list.into_iter().map(InvoiceRes::from).collect::<Vec<_>>();
    Ok(web::Json(json!(list)))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaymentRes {
    payment_request: String,
    #[serde(with = "hex::serde")]
    pub payment_hash: Vec<u8>,
    pub memo: String,
    pub timestamp: i64,
    pub value: i64,
    pub fee: i64,
    pub r#type: String,
}

impl From<invoice::Model> for PaymentRes {
    fn from(value: invoice::Model) -> Self {
        Self {
            payment_request: value.bolt11.clone(),
            payment_hash: value.payment_hash.clone(),
            memo: value.description,
            timestamp: value.generated_at,
            value: value.paid_amount / 1000, // real received amount
            r#type: "paid_invoice".to_string(),
            fee: (value.fee + value.service_fee) / 1000,
        }
    }
}

#[get("/gettxs")]
pub async fn get_txs(
    state: web::Data<AppState>,
    user: LndhubAuthedUser,
    query: web::Query<InvoicesReq>,
) -> Result<impl Responder, LndhubError> {
    let mut limit = query.limit;
    if limit == 0 {
        limit = 1000;
    }
    let list = invoice::Entity::find()
        .filter(invoice::Column::UserId.eq(user.user.id))
        .filter(invoice::Column::Type.eq(invoice::Type::Payment))
        .offset(query.offset)
        .limit(limit)
        .order_by_desc(invoice::Column::Id)
        .all(state.service.db())
        .await
        .map_err(Error::from)?;
    let list = list.into_iter().map(PaymentRes::from).collect::<Vec<_>>();
    Ok(web::Json(json!(list)))
}

#[get("/checkpayment/{payment_hash}")]
pub async fn check_payment(
    state: web::Data<AppState>,
    user: LndhubAuthedUser,
    path: web::Path<String>,
) -> Result<impl Responder, LndhubError> {
    let invoice = invoice::Entity::find()
        .filter(invoice::Column::UserId.eq(user.user.id))
        .filter(invoice::Column::Type.eq(invoice::Type::Invoice))
        .filter(
            invoice::Column::PaymentHash.eq(hex::decode(path.into_inner()).map_err(Error::from)?),
        )
        .one(state.service.db())
        .await
        .map_err(Error::from)?
        .ok_or(LndhubError::from(Error::Str("no found")))?;
    Ok(web::Json(json!({
        "paid": invoice.status == invoice::Status::Paid
    })))
}

// backwards compatibility

#[get("/getbtc")]
pub async fn get_btc(_user: LndhubAuthedUser) -> Result<impl Responder, LndhubError> {
    let list: Vec<u8> = vec![];
    Ok(web::Json(json!(list)))
}

#[get("/getpending")]
pub async fn get_pending(_user: LndhubAuthedUser) -> Result<impl Responder, LndhubError> {
    let list: Vec<u8> = vec![];
    Ok(web::Json(json!(list)))
}
