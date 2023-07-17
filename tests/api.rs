use anyhow::Result;
use migration::{Migrator, MigratorTrait};
use satsbox::{create_web_app, AppState};

pub fn get_env(key: &str) -> String {
    std::env::var(key).expect(&format!("missing env: {key}"))
}

use std::time::Duration;

use actix_rt::time::sleep;
use actix_test::read_body;
use actix_web::{
    dev::Service,
    test::{init_service, TestRequest},
    web,
};

async fn create_test_state() -> Result<AppState> {
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_filename_override(".env.test");
    // println!("{:?}", std::env::vars().collect::<Vec<_>>());
    let state = AppState::create(None::<String>, Some("SATSBOX".to_owned())).await?;
    Migrator::fresh(state.service.conn()).await?;

    Ok(state)
}

#[actix_rt::test]
async fn info() -> Result<()> {
    let state = create_test_state().await?;
    let app = init_service(create_web_app(web::Data::new(state))).await;
    sleep(Duration::from_millis(50)).await;

    let req = TestRequest::with_uri("/info").to_request();
    let res = app.call(req).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers().get(actix_http::header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let result = read_body(res).await;
    let result = String::from_utf8(result.to_vec())?;
    assert!(result.contains("id"));
    Ok(())
}
