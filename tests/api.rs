use actix_rt::time::sleep;
use actix_web::{test::init_service, web};
use anyhow::Result;
use entity::user;
use nostr_sdk::{
    secp256k1::{SecretKey, XOnlyPublicKey},
    Keys,
};
use satsbox::{create_web_app, Service};
use sea_orm::{ActiveModelTrait, Set};
use serde_json::json;
use std::{str::FromStr, time::Duration};
use util::create_test_state;

mod util;

#[actix_rt::test]
async fn info() -> Result<()> {
    let mut state = create_test_state().await?;
    state.setting.donation.privkey = Some(Keys::generate().secret_key().unwrap().into());
    state.setting.donation.amounts = vec![1_000_000, 10_000_000, 100_000_000];
    state.setting.donation.restrict_username = true;

    let app = init_service(create_web_app(web::Data::new(state))).await;
    sleep(Duration::from_millis(50)).await;

    let (val, _) = util::get(&app, "/v1/info").await?;
    // println!("val {:?}", val);
    assert!(val["node"]["id"].is_string());

    Ok(())
}

#[actix_rt::test]
async fn auth() -> Result<()> {
    let state = create_test_state().await?;
    let app = init_service(create_web_app(web::Data::new(state))).await;
    sleep(Duration::from_millis(50)).await;

    let (_val, status) = util::post(&app, "/v1/auth", json!({})).await?;
    assert_eq!(status, 401);

    let url = "http://127.0.0.1:8080/v1/auth";
    let keys = Keys::generate();

    let (val, status) = util::nostr_auth_post(
        &app,
        url,
        &keys,
        json!({
            "t": 1
        }),
    )
    .await?;

    assert_eq!(val["success"], json!(true));
    assert_eq!(status, 200);

    let (val, status) = util::nostr_auth_get(&app, url, &keys).await?;
    assert_eq!(val["success"], json!(true));
    assert_eq!(status, 200);
    Ok(())
}

#[actix_rt::test]
async fn whitelist() -> Result<()> {
    let mut state = create_test_state().await?;
    state.setting.auth.whitelist = vec![XOnlyPublicKey::from_str(
        "000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35",
    )
    .unwrap()
    .into()];
    let app = init_service(create_web_app(web::Data::new(state))).await;
    sleep(Duration::from_millis(50)).await;

    let (_val, status) = util::post(&app, "/v1/auth", json!({})).await?;
    assert_eq!(status, 401);

    let url = "http://127.0.0.1:8080/v1/auth";
    let secret_key =
        SecretKey::from_str("6b911fd37cdf5c81d4c0adb1ab7fa822ed253ab0ad9aa18d77257c88b29b718e")?;
    let alice_keys = Keys::new(secret_key);

    let (val, status) = util::nostr_auth_post(
        &app,
        url,
        &alice_keys,
        json!({
            "t": 1
        }),
    )
    .await?;

    assert_eq!(val["error"], json!(true));
    assert_eq!(status, 401);

    Ok(())
}

#[actix_rt::test]
async fn user() -> Result<()> {
    let state = web::Data::new(create_test_state().await?);
    let app = init_service(create_web_app(state.clone())).await;
    sleep(Duration::from_millis(50)).await;

    let (_val, status) = util::get(&app, "/v1/my").await?;
    assert_eq!(status, 401);

    let my_url = "http://127.0.0.1:8080/v1/my";
    let update_username_url = "http://127.0.0.1:8080/v1/update_username";

    let keys = Keys::generate();
    let pubkey = keys.public_key().serialize().to_vec();

    let (val, status) = util::nostr_auth_get(&app, my_url, &keys).await?;
    assert_eq!(status, 200);
    assert_eq!(val["user"]["pubkey"], json!(keys.public_key().to_string()));
    assert_eq!(val["user"]["balance"], json!(0));
    assert_eq!(val["user"]["allow_update_username"], json!(true));
    assert!(val["user"]["lndhub"]["url"].is_null());
    assert!(val["user"]["username"].is_null());

    let user = state.service.get_or_create_user(pubkey.clone()).await?;
    let amount = 100_000;
    state
        .service
        .admin_adjust_user_balance(&user, amount, None)
        .await?;

    util::nostr_auth_post(
        &app,
        update_username_url,
        &keys,
        json!({ "username": "tester" }),
    )
    .await?;

    let (val, _status) = util::nostr_auth_get(&app, my_url, &keys).await?;
    assert_eq!(val["user"]["balance"], json!(amount));
    assert_eq!(val["user"]["username"], json!("tester"));

    util::nostr_auth_post(&app, update_username_url, &keys, json!({})).await?;
    let (val, _status) = util::nostr_auth_get(&app, my_url, &keys).await?;
    assert!(val["user"]["username"].is_null());

    // invalid
    let (val, status) = util::nostr_auth_post(
        &app,
        update_username_url,
        &keys,
        json!({ "username": "tester&" }),
    )
    .await?;
    assert_eq!(val["error"], json!(true));
    assert_eq!(status, 400);
    assert!(val["message"].as_str().unwrap().contains("characters"));

    let (val, status) =
        util::nostr_auth_post(&app, update_username_url, &keys, json!({ "username": "t" })).await?;
    assert_eq!(val["error"], json!(true));
    assert_eq!(status, 400);
    assert!(val["message"].as_str().unwrap().contains("less"));

    let (val, status) = util::nostr_auth_post(
        &app,
        update_username_url,
        &keys,
        json!({ "username": "t11111111111111111111111111111" }),
    )
    .await?;
    assert_eq!(val["error"], json!(true));
    assert_eq!(status, 400);
    assert!(val["message"].as_str().unwrap().contains("greater"));
    Ok(())
}

#[actix_rt::test]
async fn donate_user() -> Result<()> {
    let mut state = create_test_state().await?;
    state.setting.donation.privkey = Some(Keys::generate().secret_key().unwrap().into());
    state.setting.donation.amounts = vec![1_000_000, 10_000_000, 100_000_000];
    state.setting.donation.restrict_username = true;

    let state = web::Data::new(state);
    let app = init_service(create_web_app(state.clone())).await;
    sleep(Duration::from_millis(50)).await;

    let my_url = "http://127.0.0.1:8080/v1/my";
    let update_username_url = "http://127.0.0.1:8080/v1/update_username";

    let keys = Keys::generate();
    let pubkey = keys.public_key().serialize().to_vec();

    let (val, status) = util::nostr_auth_get(&app, my_url, &keys).await?;
    assert_eq!(status, 200);
    assert_eq!(val["user"]["allow_update_username"], json!(false));

    let (val, _) = util::nostr_auth_post(
        &app,
        update_username_url,
        &keys,
        json!({ "username": "tt" }),
    )
    .await?;
    assert_eq!(val["error"], json!(true));
    assert!(val["message"].as_str().unwrap().contains("allowed"));

    update_donate_amount(&state.service, pubkey.clone(), 1_000_000).await?;
    let (val, status) = util::nostr_auth_get(&app, my_url, &keys).await?;
    assert_eq!(status, 200);
    assert_eq!(val["user"]["allow_update_username"], json!(true));
    assert_eq!(val["user"]["allow_update_username_min_chars"], json!(4));

    update_donate_amount(&state.service, pubkey.clone(), 1_100_000).await?;
    let (val, _) = util::nostr_auth_get(&app, my_url, &keys).await?;
    assert_eq!(val["user"]["allow_update_username_min_chars"], json!(4));

    update_donate_amount(&state.service, pubkey.clone(), 100_100_000).await?;
    let (val, _) = util::nostr_auth_get(&app, my_url, &keys).await?;
    assert_eq!(val["user"]["allow_update_username_min_chars"], json!(2));

    update_donate_amount(&state.service, pubkey.clone(), 10_100_000).await?;
    let (val, _) = util::nostr_auth_get(&app, my_url, &keys).await?;
    assert_eq!(val["user"]["allow_update_username_min_chars"], json!(3));

    let (val, _) = util::nostr_auth_post(
        &app,
        update_username_url,
        &keys,
        json!({ "username": "tt" }),
    )
    .await?;
    assert_eq!(val["error"], json!(true));
    assert!(val["message"].as_str().unwrap().contains("less"));

    let (val, _) = util::nostr_auth_post(
        &app,
        update_username_url,
        &keys,
        json!({ "username": "ttt" }),
    )
    .await?;
    assert_eq!(val["success"], json!(true));
    Ok(())
}

async fn update_donate_amount(service: &Service, pubkey: Vec<u8>, amount: i64) -> Result<()> {
    let user = service.get_or_create_user(pubkey.clone()).await?;
    user::ActiveModel {
        id: Set(user.id),
        donate_amount: Set(amount),
        ..Default::default()
    }
    .update(service.db())
    .await?;
    Ok(())
}
#[actix_rt::test]
async fn reset_lndhub() -> Result<()> {
    let state = web::Data::new(create_test_state().await?);
    let app = init_service(create_web_app(state.clone())).await;
    sleep(Duration::from_millis(50)).await;

    let (_val, status) = util::post(&app, "/v1/reset_lndhub", json!({})).await?;
    assert_eq!(status, 401);

    let url = "http://127.0.0.1:8080/v1/reset_lndhub";
    let keys = Keys::generate();
    // let pubkey = keys.public_key().serialize().to_vec();

    let (val, status) =
        util::nostr_auth_post(&app, url, &keys, json!({ "disable": false })).await?;
    assert_eq!(status, 200);
    assert_eq!(val["lndhub"]["login"], json!(keys.public_key().to_string()));
    assert!(val["lndhub"]["password"].is_string());
    let lndhub_url = format!(
        "lndhub://{}:{}@http://127.0.0.1:8080",
        keys.public_key().to_string(),
        val["lndhub"]["password"].as_str().unwrap()
    );
    assert_eq!(val["lndhub"]["url"], json!(lndhub_url));

    let (val, _status) = util::nostr_auth_get(&app, "http://127.0.0.1:8080/v1/my", &keys).await?;
    assert!(val["user"]["lndhub"]["url"].is_string());

    Ok(())
}
