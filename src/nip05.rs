//! nip05 api

use std::collections::HashMap;

use crate::{AppState, Error, Result};
use actix_web::{get, web, Responder};

use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct InfoReq {
    pub name: String,
}

#[get("/nostr.json")]
pub async fn info(
    state: web::Data<AppState>,
    query: web::Query<InfoReq>,
) -> Result<impl Responder, Error> {
    let name = query.name.clone();
    let user = state.service.get_user_by_name(name.clone()).await?;
    // .ok_or(Error::Str("invalid user"))?
    let mut map = HashMap::new();
    if let Some(user) = user {
        map.insert(name, hex::encode(user.pubkey));
    }
    Ok(web::Json(json!({"names": map})))
}
