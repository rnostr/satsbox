use actix_rt::time::sleep;
use actix_web::{test::init_service, web};
use anyhow::Result;
use satsbox::create_web_app;
use serde_json::json;
use std::time::Duration;
use util::create_test_state;

mod util;

const PUBKEY: &str = "4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd20";

#[actix_rt::test]
async fn nip05() -> Result<()> {
    let state = create_test_state().await?;
    let pubkey = hex::decode(PUBKEY)?;

    let user = state.service.get_or_create_user(pubkey.clone()).await?;
    state
        .service
        .update_user_name(user.id, Some("admin".to_owned()))
        .await?;

    let app = init_service(create_web_app(web::Data::new(state))).await;
    sleep(Duration::from_millis(50)).await;

    let (val, _) = util::get(&app, "/.well-known/nostr.json?name=admin").await?;
    assert!(val["names"]["admin"].is_string());
    assert_eq!(val["names"]["admin"], json!(PUBKEY));

    let (val, _) = util::get(&app, "/.well-known/nostr.json?name=unknown").await?;
    assert!(val["names"]["unknown"].is_null());
    Ok(())
}
