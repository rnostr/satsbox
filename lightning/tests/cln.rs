mod lightning;
use std::time::Duration;

use anyhow::Result;
mod util;

macro_rules! test_method {
    ($t:ident) => {
        #[tokio::test]
        async fn $t() -> Result<()> {
            dotenvy::dotenv()?;
            let client = util::connect_cln(None).await?;
            lightning::$t(&client).await?;
            Ok(())
        }
    };
}

#[tokio::test]
async fn timeout() -> Result<()> {
    dotenvy::dotenv()?;
    let client = util::connect_cln(Some(Duration::from_nanos(10))).await?;
    lightning::timeout(&client).await?;
    Ok(())
}

test_method!(get_info);
test_method!(create_invoice);
