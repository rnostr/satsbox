mod lightning;
use anyhow::Result;
use lightning::get_env;
use lightning_client::Lnd;

async fn connect() -> Result<Lnd> {
    let url = get_env("LT_LND__URL");
    let cert_file = get_env("LT_LND__CERT");
    let macaroon_file = get_env("LT_LND__MACAROON");
    Ok(Lnd::connect(url.to_owned(), cert_file, macaroon_file).await?)
}

macro_rules! test_method {
    ($t:ident) => {
        #[tokio::test]
        async fn $t() -> Result<()> {
            dotenvy::dotenv()?;
            let mut client = connect().await?;
            lightning::$t(&mut client).await?;
            Ok(())
        }
    };
}

test_method!(get_info);
test_method!(create_invoice);
