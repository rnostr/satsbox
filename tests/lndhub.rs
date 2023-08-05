use actix_rt::time::sleep;
use actix_web::{
    test::{call_service, init_service, read_body_json, TestRequest},
    web,
};
use anyhow::Result;
use migration::{Migrator, MigratorTrait};
use satsbox::{create_web_app, AppState};
use std::time::Duration;

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

    let req = TestRequest::with_uri("/getinfo").to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers().get(actix_http::header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    read_body_json::<satsbox::lndhub::Info, _>(res).await;
    Ok(())
}

#[actix_rt::test]
async fn auth() -> Result<()> {
    let state = create_test_state().await?;
    let app = init_service(create_web_app(web::Data::new(state))).await;
    sleep(Duration::from_millis(50)).await;

    let req = TestRequest::with_uri("/getinfo").to_request();
    let res = call_service(&app, req).await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers().get(actix_http::header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    read_body_json::<satsbox::lndhub::Info, _>(res).await;
    Ok(())
}
