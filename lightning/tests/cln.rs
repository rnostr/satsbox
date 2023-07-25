mod lightning;
use anyhow::Result;
use lightning::get_env;
use lightning_client::Cln;

async fn connect() -> Result<Cln> {
    let url = get_env("LT_CLN__URL");
    let ca = get_env("LT_CLN__CA");
    let client = get_env("LT_CLN__CLIENT");
    let client_key = get_env("LT_CLN__CLIENT_KEY");
    Ok(Cln::connect(url, ca, client, client_key).await?)
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
test_method!(track_payment);
