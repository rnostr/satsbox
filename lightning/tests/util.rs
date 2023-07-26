#![allow(unused)]

use anyhow::Result;
use lightning_client::{Cln, Lnd};
use std::time::Duration;

pub fn get_env(key: &str) -> String {
    std::env::var(key).expect(&format!("missing env: {key}"))
}

pub async fn connect_cln(timeout: Option<Duration>) -> Result<Cln> {
    let url = get_env("LT_CLN__URL");
    let ca = get_env("LT_CLN__CA");
    let client = get_env("LT_CLN__CLIENT");
    let client_key = get_env("LT_CLN__CLIENT_KEY");
    Ok(Cln::connect(url, ca, client, client_key, timeout).await?)
}

pub async fn connect_lnd(timeout: Option<Duration>) -> Result<Lnd> {
    let url = get_env("LT_LND__URL");
    let cert_file = get_env("LT_LND__CERT");
    let macaroon_file = get_env("LT_LND__MACAROON");
    Ok(Lnd::connect(url.to_owned(), cert_file, macaroon_file, timeout).await?)
}
