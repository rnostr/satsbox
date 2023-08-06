use actix_rt::time::sleep;
use actix_web::{
    test::{call_service, init_service},
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

    let req = util::post(
        "/auth",
        json!({
            "login": pubkey_str,
        }),
    )
    .to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    let val = util::json(res).await;
    assert_eq!(val["error"], json!(true));
    assert_eq!(val["code"], json!(8));

    let req = util::post(
        "/auth",
        json!({
            "refresh_token": "invalid",
        }),
    )
    .to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    let val = util::json(res).await;
    assert_eq!(val["error"], json!(true));

    let req = util::post(
        "/auth",
        json!({
            "login": pubkey_str,
            "password": password,
        }),
    )
    .to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    let val = util::json(res).await;

    assert!(val["access_token"].is_string());
    assert!(val["refresh_token"].is_string());

    let refresh_token = val["refresh_token"].as_str().unwrap().to_owned();

    let req = util::post(
        "/auth",
        json!({
            "refresh_token": refresh_token,
        }),
    )
    .to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    let val = util::json(res).await;
    assert!(val["access_token"].is_string());
    assert!(val["refresh_token"].is_string());

    let access_token = val["access_token"].as_str().unwrap().to_owned();

    let req = util::get("/getinfo").to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    let val = util::json(res).await;
    assert_eq!(val["code"], json!(1));

    let req = util::auth_get("/getinfo", &access_token).to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    let val = util::json(res).await;
    assert!(val["identity_pubkey"].is_string());

    // disable lndhub by remove password
    state.service.update_user_password(user.id, None).await?;

    let req = util::auth_get("/getinfo", &access_token).to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    let val = util::json(res).await;
    assert_eq!(val["code"], json!(1));

    let req = util::post(
        "/auth",
        json!({
            "login": pubkey_str,
            "password": password,
        }),
    )
    .to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    let val = util::json(res).await;
    assert_eq!(val["code"], json!(1));

    Ok(())
}
