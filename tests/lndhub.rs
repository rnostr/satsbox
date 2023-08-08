use actix_http::Request;
use actix_rt::time::sleep;
use actix_web::{
    body::MessageBody,
    dev::{Service, ServiceResponse},
    test::init_service,
    web,
};
use anyhow::Result;
use migration::{Migrator, MigratorTrait};
use satsbox::{create_web_app, AppState};
use serde_json::json;
use std::time::Duration;
mod util;

async fn create_test_state() -> Result<AppState> {
    dotenvy::from_filename(".test.env")?;
    let state = AppState::create(None::<String>, Some("SATSBOX".to_owned())).await?;
    Migrator::fresh(state.service.db()).await?;
    Ok(state)
}

#[actix_rt::test]
async fn auth() -> Result<()> {
    let state = web::Data::new(create_test_state().await?);
    let app = init_service(create_web_app(state.clone())).await;
    sleep(Duration::from_millis(50)).await;

    let pubkey_str = "000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35".to_string();
    let password = "random password".to_string();
    let pubkey = hex::decode(&pubkey_str)?;
    let user = state.service.get_or_create_user(pubkey.clone()).await?;
    state
        .service
        .update_user_password(user.id, Some(password.clone()))
        .await?;

    let (val, status) = util::post(
        &app,
        "/auth",
        json!({
            "login": pubkey_str,
        }),
    )
    .await?;
    assert_eq!(status, 200);
    assert_eq!(val["error"], json!(true));
    assert_eq!(val["code"], json!(8));

    let (val, status) = util::post(
        &app,
        "/auth",
        json!({
            "refresh_token": "invalid",
        }),
    )
    .await?;
    assert_eq!(status, 200);
    assert_eq!(val["error"], json!(true));

    let (val, status) = util::post(
        &app,
        "/auth",
        json!({
            "login": pubkey_str,
            "password": password,
        }),
    )
    .await?;
    assert_eq!(status, 200);
    assert!(val["access_token"].is_string());
    assert!(val["refresh_token"].is_string());

    let refresh_token = val["refresh_token"].as_str().unwrap().to_owned();

    let (val, status) = util::post(
        &app,
        "/auth",
        json!({
            "refresh_token": refresh_token,
        }),
    )
    .await?;
    assert_eq!(status, 200);
    assert!(val["access_token"].is_string());
    assert!(val["refresh_token"].is_string());

    let access_token = val["access_token"].as_str().unwrap().to_owned();

    let (val, status) = util::get(&app, "/getinfo").await?;
    assert_eq!(status, 200);
    assert_eq!(val["code"], json!(1));

    let (val, status) = util::auth_get(&app, "/getinfo", &access_token).await?;
    assert_eq!(status, 200);
    assert!(val["identity_pubkey"].is_string());

    // disable lndhub by remove password
    state.service.update_user_password(user.id, None).await?;

    let (val, status) = util::auth_get(&app, "/getinfo", &access_token).await?;
    assert_eq!(status, 200);
    assert_eq!(val["code"], json!(1));

    let (val, status) = util::post(
        &app,
        "/auth",
        json!({
            "login": pubkey_str,
            "password": password,
        }),
    )
    .await?;
    assert_eq!(status, 200);
    assert_eq!(val["code"], json!(1));

    Ok(())
}

pub async fn create_authed_app(
    balance: i64,
) -> Result<(
    impl Service<Request, Response = ServiceResponse<impl MessageBody>, Error = actix_web::Error>,
    web::Data<AppState>,
    String,
)> {
    let mut state = create_test_state().await.unwrap();
    state.service.self_payment = true;
    let state = web::Data::new(state);

    let app = init_service(create_web_app(state.clone())).await;
    sleep(Duration::from_millis(50)).await;

    let pubkey_str = "000003a91077fc049b8371e7a523fb5dfd9daff4522aa3f510d02bc9f490ca35".to_string();
    let password = "random password".to_string();
    let pubkey = hex::decode(&pubkey_str)?;
    let user = state.service.get_or_create_user(pubkey.clone()).await?;
    state
        .service
        .update_user_password(user.id, Some(password.clone()))
        .await?;
    // 5k sats
    state
        .service
        .admin_adjust_user_balance(&user, balance, None)
        .await?;

    let (val, _) = util::post(
        &app,
        "/auth",
        json!({
            "login": pubkey_str,
            "password": password,
        }),
    )
    .await?;
    assert!(val["access_token"].is_string());
    let access_token = val["access_token"].as_str().unwrap().to_owned();

    Ok((app, state, access_token))
}

#[actix_rt::test]
async fn balance() -> Result<()> {
    let balance = 5_000_000;
    let (app, _state, access_token) = create_authed_app(balance).await?;

    let (val, _) = util::auth_get(&app, "/balance", &access_token).await?;
    assert_eq!(val["BTC"]["AvailableBalance"], json!(balance / 1000));
    Ok(())
}

#[actix_rt::test]
async fn invoice() -> Result<()> {
    let balance = 5_000_000; // msats
    let (app, _state, access_token) = create_authed_app(balance).await?;

    let amt = 1_000;
    let (val, _) = util::auth_post(
        &app,
        "/addinvoice",
        &access_token,
        json!({
            "memo": "test",
            "amt": amt.to_string(), // lndhub use string
        }),
    )
    .await?;
    assert!(val["payment_request"].is_string());
    assert!(val["r_hash"].is_string());
    assert_eq!(val["payment_request"], val["pay_req"]);
    assert_eq!(val["amt"], json!(amt));

    sleep(Duration::from_secs(2)).await;

    util::auth_post(
        &app,
        "/addinvoice",
        &access_token,
        json!({
            "memo": "test",
            "amt": amt, // test number
        }),
    )
    .await?;

    // list
    let (val, _) = util::auth_get(&app, "/getuserinvoices", &access_token).await?;
    assert!(val.is_array());
    let ar = val.as_array().unwrap();
    assert_eq!(ar.len(), 2);

    let (val, _) = util::auth_get(&app, "/getuserinvoices?limit=1", &access_token).await?;
    assert!(val.is_array());
    let ar = val.as_array().unwrap();
    assert_eq!(ar.len(), 1);

    Ok(())
}

#[actix_rt::test]
async fn payment() -> Result<()> {
    let balance = 5_000_000; // msats

    let (app, state, access_token) = create_authed_app(balance).await?;

    let amt = 1_000_000;
    let (val, _) = util::auth_post(
        &app,
        "/addinvoice",
        &access_token,
        json!({
            "memo": "test",
            "amt": amt / 1000,
        }),
    )
    .await?;
    assert!(val["payment_request"].is_string());
    assert!(val["r_hash"].is_string());
    let payment_hash = val["r_hash"].as_str().unwrap().to_owned();
    let bolt11 = val["payment_request"].as_str().unwrap().to_owned();

    // checkpayment
    let (val, _) = util::auth_get(
        &app,
        &format!("/checkpayment/{}", payment_hash),
        &access_token,
    )
    .await?;
    assert_eq!(val["paid"], json!(false));

    // self payment

    let (val, _) = util::auth_post(
        &app,
        "/payinvoice",
        &access_token,
        json!({
            "invoice": bolt11,
            "amount": 0,
        }),
    )
    .await?;
    // println!("val: {:?}", val);
    assert_eq!(val["num_satoshis"], json!(amt / 1000));
    assert_eq!(val["payment_hash"], json!(payment_hash));

    // checkpayment
    let (val, _) = util::auth_get(
        &app,
        &format!("/checkpayment/{}", payment_hash),
        &access_token,
    )
    .await?;
    assert_eq!(val["paid"], json!(true));

    // list
    let (val, _) = util::auth_get(&app, "/getuserinvoices", &access_token).await?;
    assert!(val.is_array());
    let ar = val.as_array().unwrap();
    assert_eq!(ar.len(), 1);
    assert_eq!(ar[0]["ispaid"], json!(true));

    let (val, _) = util::auth_get(&app, "/gettxs", &access_token).await?;
    assert!(val.is_array());
    let ar = val.as_array().unwrap();
    assert_eq!(ar.len(), 1);
    assert_eq!(ar[0]["payment_hash"], json!(payment_hash));

    let fee = state.setting.fee.cal(amt, true);
    let (val, _) = util::auth_get(&app, "/balance", &access_token).await?;
    assert_eq!(
        val["BTC"]["AvailableBalance"],
        json!((balance - fee.0 - fee.1) / 1000)
    );
    Ok(())
}
