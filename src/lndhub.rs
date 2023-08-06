//! lnd hub api

use crate::{
    jwt_auth::{AuthToken, AuthedUser},
    AppState, Error, Result,
};
use actix_http::Payload;
use actix_web::{
    get, http::StatusCode, post, web, FromRequest, HttpRequest, HttpResponse, ResponseError,
};
use entity::user;
use lightning_client::lightning;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{future::Future, pin::Pin};

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
            Err(Error::Unauthorized.into())
        }
    }
}

impl FromRequest for LndhubAuthedUser {
    type Error = LndhubError;
    type Future = Pin<Box<dyn Future<Output = Result<LndhubAuthedUser, LndhubError>>>>;
    fn from_request(req: &HttpRequest, pl: &mut Payload) -> Self::Future {
        let fut = AuthedUser::from_request(req, pl);
        Box::pin(async move {
            if let Ok(user) = fut.await {
                return LndhubAuthedUser::try_from(user);
            }
            Err(Error::Unauthorized.into())
        })
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
            LndhubError::Base(Error::Unauthorized) => 1,
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

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_info).service(auth);
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
pub async fn get_info(
    state: web::Data<AppState>,
    _user: LndhubAuthedUser,
) -> Result<HttpResponse, LndhubError> {
    let info = state.service.info().await?;
    Ok(HttpResponse::Ok().json(Info::from(info)))
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

    let refresh_token = AuthToken::generate(
        user.id,
        state.setting.auth.refresh_token_expiry,
        state.setting.auth.secret.as_bytes(),
    )?;

    let access_token = AuthToken::generate(
        user.id,
        state.setting.auth.access_token_expiry,
        state.setting.auth.secret.as_bytes(),
    )?;

    Ok(HttpResponse::Ok().json(AuthRes {
        refresh_token,
        access_token,
    }))
}
