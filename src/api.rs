//! http api

use crate::{auth, AppState, Error, Result};
use actix_web::{get, post, web, HttpResponse, Responder};

use serde_json::{json, Value};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(info).service(post_auth).service(get_auth);
}

#[get("/info")]
pub async fn info(state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let info = state.service.info().await?;
    Ok(HttpResponse::Ok().json(info))
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
