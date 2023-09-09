#![allow(unused)]

use actix_http::Request;
use actix_web::{
    body::MessageBody,
    dev::{Service, ServiceResponse},
    http::{header::AUTHORIZATION, Method},
    test::{call_service, read_body_json, TestRequest},
};
use anyhow::Result;
use base64::engine::{general_purpose, Engine};
use migration::{Migrator, MigratorTrait};
use nostr_sdk::{EventBuilder, Keys, Kind, Tag};
use satsbox::{
    setting::{Lightning, Setting},
    sha256, AppState,
};
use serde_json::Value;

pub async fn create_test_state() -> Result<AppState> {
    dotenvy::from_filename(".test.env")?;
    let state = AppState::create(None::<String>, Some("SATSBOX".to_owned())).await?;
    Migrator::fresh(state.service.db()).await?;
    Ok(state)
}

pub async fn create_test_state2(lightning: Option<Lightning>) -> Result<AppState> {
    dotenvy::from_filename(".test.env")?;
    let mut setting = Setting::from_env("SATSBOX".to_owned())?;
    if let Some(lightning) = lightning {
        setting.lightning = lightning;
    }
    let state = AppState::from_setting(setting).await?;
    Migrator::fresh(state.service.db()).await?;
    Ok(state)
}

pub async fn get(
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    path: &str,
) -> Result<(Value, u16)> {
    let req = get_req(path);
    call(req, app).await
}

pub async fn nostr_auth_get(
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    url: &str,
    keys: &Keys,
) -> Result<(Value, u16)> {
    let event = EventBuilder::new(
        Kind::from(27235),
        "",
        &vec![
            Tag::try_from(vec!["u", url])?,
            Tag::try_from(vec!["method", "GET"])?,
        ],
    )
    .to_event(keys)?;
    let token = general_purpose::STANDARD.encode(event.as_json());
    let req = auth_get_req(url, format!("Nostr {}", token));
    call(req, app).await
}

pub async fn nostr_auth_post(
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    url: &str,
    keys: &Keys,
    data: Value,
) -> Result<(Value, u16)> {
    let hash = sha256(serde_json::to_string(&data)?);
    let event = EventBuilder::new(
        Kind::from(27235),
        "",
        &vec![
            Tag::try_from(vec!["u", url])?,
            Tag::try_from(vec!["method", "POST"])?,
            Tag::try_from(vec!["payload", &hex::encode(&hash)])?,
        ],
    )
    .to_event(keys)?;
    let token = general_purpose::STANDARD.encode(event.as_json());
    let req = auth_post_req(url, format!("Nostr {}", token), data);
    call(req, app).await
}

pub async fn auth_get(
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    path: &str,
    token: &String,
) -> Result<(Value, u16)> {
    let req = auth_get_req(path, format!("Bearer {}", token));
    call(req, app).await
}

pub async fn auth_post(
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    path: &str,
    token: &String,
    data: Value,
) -> Result<(Value, u16)> {
    let req = auth_post_req(path, format!("Bearer {}", token), data);
    call(req, app).await
}

pub async fn post(
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    path: &str,
    data: Value,
) -> Result<(Value, u16)> {
    let req = post_req(path, data);
    call(req, app).await
}

pub async fn call(
    req: TestRequest,
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
) -> Result<(Value, u16)> {
    let res = call_service(&app, req.to_request()).await;
    let status = res.status().as_u16();
    let val = json(res).await;
    Ok((val, status))
}

pub fn auth_get_req(path: &str, auth: String) -> TestRequest {
    TestRequest::with_uri(path).insert_header((AUTHORIZATION, auth))
}

pub fn get_req(path: &str) -> TestRequest {
    TestRequest::with_uri(path)
}

pub fn auth_post_req(path: &str, auth: String, data: Value) -> TestRequest {
    TestRequest::with_uri(path)
        .method(Method::POST)
        .set_json(data)
        .insert_header((AUTHORIZATION, auth))
}

pub fn post_req(path: &str, data: Value) -> TestRequest {
    TestRequest::with_uri(path)
        .method(Method::POST)
        .set_json(data)
}

pub async fn json<B>(res: ServiceResponse<B>) -> Value
where
    B: MessageBody,
{
    assert_eq!(
        res.headers()
            .get(actix_web::http::header::CONTENT_TYPE)
            .unwrap(),
        "application/json"
    );
    read_body_json::<Value, _>(res).await
}
