//! lnd hub api

use crate::{
    jwt_auth::{AuthToken, AuthedUser},
    AppState, Error, Result,
};
use actix_http::Payload;
use actix_web::{get, post, web, FromRequest, HttpRequest, HttpResponse};
use entity::user;
use lightning_client::lightning;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

/// Lndhub authed user.
/// Requires password already set.

#[derive(Debug)]
struct LndhubAuthedUser {
    pub user: user::Model,
}

impl TryFrom<AuthedUser> for LndhubAuthedUser {
    type Error = Error;
    fn try_from(user: AuthedUser) -> Result<Self> {
        if user.user.password.is_some() {
            Ok(LndhubAuthedUser { user: user.user })
        } else {
            Err(Error::Unauthorized)
        }
    }
}

impl FromRequest for LndhubAuthedUser {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<LndhubAuthedUser>>>>;
    fn from_request(req: &HttpRequest, pl: &mut Payload) -> Self::Future {
        let fut = AuthedUser::from_request(req, pl);
        Box::pin(async move {
            if let Ok(user) = fut.await {
                return LndhubAuthedUser::try_from(user);
            }
            Err(Error::Unauthorized)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LndhubError {
    error: bool,
    code: i8,
    message: String,
}

impl LndhubError {
    pub fn new(code: i8, message: String) -> Self {
        Self {
            error: true,
            code,
            message,
        }
    }
    pub fn bad_auth() -> Self {
        Self::new(1, "bad auth".to_owned())
    }

    pub fn not_enoug_balance() -> Self {
        Self::new(2, "not enough balance".to_owned())
    }

    pub fn not_a_valid_invoice() -> Self {
        Self::new(4, "not a valid invoice".to_owned())
    }
    pub fn lnd() -> Self {
        Self::new(7, "LND failue".to_owned())
    }
    pub fn server_error() -> Self {
        Self::new(6, "Something went wrong. Please try again later".to_owned())
    }
    pub fn bad_arguments() -> Self {
        Self::new(8, "Bad arguments".to_owned())
    }
    pub fn try_again_later() -> Self {
        Self::new(
            9,
            "Your previous payment is in transit. Try again later".to_owned(),
        )
    }
    pub fn payment_failed() -> Self {
        Self::new(
            10,
            "Payment failed. Does the receiver have enough inbound capacity?".to_owned(),
        )
    }
    pub fn sunset() -> Self {
        Self::new(
            11,
            "This lightning instance is not accepting any more users".to_owned(),
        )
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_info);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
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
}

impl From<lightning::Info> for Info {
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
        }
    }
}

#[get("/getinfo")]
pub async fn get_info(state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let info = state.service.info().await?;
    Ok(HttpResponse::Ok().json(Info::from(info)))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ReqAuth {
    login: String,
    password: String,
    refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ResAuth {
    refresh_token: String,
    access_token: String,
}

#[post("/auth")]
pub async fn auth(
    state: web::Data<AppState>,
    data: web::Json<ReqAuth>,
) -> Result<HttpResponse, Error> {
    if !data.refresh_token.is_empty() {
        // auth from refresh token
        let user = AuthedUser::from_token(&data.refresh_token, &state).await?;
        let user = LndhubAuthedUser::try_from(user)?;
        let refresh_token = AuthToken::generate(
            user.user.id,
            state.setting.auth.refresh_token_expiry,
            state.setting.auth.secret.as_bytes(),
        )?;

        let access_token = AuthToken::generate(
            user.user.id,
            state.setting.auth.access_token_expiry,
            state.setting.auth.secret.as_bytes(),
        )?;

        return Ok(HttpResponse::Ok().json(ResAuth {
            refresh_token,
            access_token,
        }));
    } else if !data.login.is_empty() && !data.password.is_empty() {
        todo!()
    } else {
        Ok(HttpResponse::Ok().json(LndhubError::bad_arguments()))
    }
}
