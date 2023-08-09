#![allow(unused)]

use actix_http::Request;

use actix_web::{
    body::MessageBody,
    dev::{Service, ServiceResponse},
    http::{header::AUTHORIZATION, Method},
    test::{call_service, read_body_json, TestRequest},
};
use anyhow::Result;
use serde_json::Value;

pub async fn get(
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    path: &str,
) -> Result<(Value, u16)> {
    let req = get_req(path);
    call(req, app).await
}

pub async fn auth_get(
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    path: &str,
    token: &String,
) -> Result<(Value, u16)> {
    let req = auth_get_req(path, token);
    call(req, app).await
}

pub async fn auth_post(
    app: impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    path: &str,
    token: &String,
    data: Value,
) -> Result<(Value, u16)> {
    let req = auth_post_req(path, token, data);
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

pub fn auth_get_req(path: &str, token: &String) -> TestRequest {
    TestRequest::with_uri(path).insert_header((AUTHORIZATION, format!("Bearer {}", token)))
}

pub fn get_req(path: &str) -> TestRequest {
    TestRequest::with_uri(path)
}

pub fn auth_post_req(path: &str, token: &String, data: Value) -> TestRequest {
    TestRequest::with_uri(path)
        .method(Method::POST)
        .set_json(data)
        .insert_header((AUTHORIZATION, format!("Bearer {}", token)))
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
