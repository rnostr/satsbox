use actix_rt::time::sleep;
use actix_web::{http::header::AUTHORIZATION, test::init_service, web};
use anyhow::Result;
use base64::engine::{general_purpose, Engine};
use migration::{Migrator, MigratorTrait};
use nostr_sdk::{secp256k1::SecretKey, EventBuilder, Keys, Kind, Tag};
use satsbox::{create_web_app, sha256, AppState};
use serde_json::json;
use std::{str::FromStr, time::Duration};

mod util;

const ALICE_SK: &str = "6b911fd37cdf5c81d4c0adb1ab7fa822ed253ab0ad9aa18d77257c88b29b718e";

async fn create_test_state() -> Result<AppState> {
    dotenvy::from_filename(".test.env")?;
    let state = AppState::create(None::<String>, Some("SATSBOX".to_owned())).await?;
    Migrator::fresh(state.service.db()).await?;
    Ok(state)
}

#[actix_rt::test]
async fn info() -> Result<()> {
    let state = create_test_state().await?;
    let app = init_service(create_web_app(web::Data::new(state))).await;
    sleep(Duration::from_millis(50)).await;

    let (val, _) = util::get(&app, "/v1/info").await?;
    assert!(val["id"].is_string());

    Ok(())
}

#[actix_rt::test]
async fn auth() -> Result<()> {
    let state = create_test_state().await?;
    let app = init_service(create_web_app(web::Data::new(state))).await;
    sleep(Duration::from_millis(50)).await;

    let (_val, status) = util::post(&app, "/v1/auth", json!({})).await?;
    assert_eq!(status, 401);

    let hash = sha256("{\"t\":1}");
    let url = "http://127.0.0.1:8080/v1/auth";
    let secret_key = SecretKey::from_str(ALICE_SK)?;
    let alice_keys = Keys::new(secret_key);

    let event = EventBuilder::new(
        Kind::from(27235),
        "",
        &vec![
            Tag::try_from(vec!["u", url])?,
            Tag::try_from(vec!["method", "POST"])?,
            Tag::try_from(vec!["payload", &hex::encode(&hash)])?,
        ],
    )
    .to_event(&alice_keys)?;
    let encoded = general_purpose::STANDARD.encode(event.as_json());

    let (val, status) = util::call(
        util::post_req(
            url,
            json!({
                "t": 1
            }),
        )
        .insert_header((AUTHORIZATION, format!("Nostr {}", encoded))),
        &app,
    )
    .await?;

    assert_eq!(val["success"], json!(true));
    assert_eq!(status, 200);

    let event = EventBuilder::new(
        Kind::from(27235),
        "",
        &vec![
            Tag::try_from(vec!["u", url])?,
            Tag::try_from(vec!["method", "GET"])?,
        ],
    )
    .to_event(&alice_keys)?;
    let encoded = general_purpose::STANDARD.encode(event.as_json());

    let (val, status) = util::call(
        util::get_req(url).insert_header((AUTHORIZATION, format!("Nostr {}", encoded))),
        &app,
    )
    .await?;

    assert_eq!(val["success"], json!(true));
    assert_eq!(status, 200);
    Ok(())
}
