#![allow(unused)]

use actix_http::{body::MessageBody, header::AUTHORIZATION, Method};
use actix_web::{
    dev::ServiceResponse,
    test::{read_body_json, TestRequest},
};
use anyhow::Result;
use serde_json::Value;

pub fn auth_get(path: &str, token: &String) -> TestRequest {
    TestRequest::with_uri(path).insert_header((AUTHORIZATION, format!("Bearer {}", token)))
}

pub fn get(path: &str) -> TestRequest {
    TestRequest::with_uri(path)
}

pub fn auth_post(path: &str, token: &String, data: Value) -> TestRequest {
    TestRequest::with_uri(path)
        .method(Method::POST)
        .set_json(data)
        .insert_header((AUTHORIZATION, format!("Bearer {}", token)))
}

pub fn post(path: &str, data: Value) -> TestRequest {
    TestRequest::with_uri(path)
        .method(Method::POST)
        .set_json(data)
}

pub async fn json<B>(res: ServiceResponse<B>) -> Value
where
    B: MessageBody,
{
    assert_eq!(
        res.headers().get(actix_http::header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    read_body_json::<Value, _>(res).await
}
