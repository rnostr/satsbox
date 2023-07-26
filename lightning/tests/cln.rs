mod lightning;
use anyhow::Result;
mod util;

macro_rules! test_method {
    ($t:ident) => {
        #[tokio::test]
        async fn $t() -> Result<()> {
            dotenvy::dotenv()?;
            let client = util::connect_cln().await?;
            lightning::$t(&client).await?;
            Ok(())
        }
    };
}

test_method!(get_info);
test_method!(create_invoice);
