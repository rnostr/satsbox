mod lightning;
use anyhow::Result;
use std::time::Duration;
mod util;

macro_rules! test_method {
    ($t:ident) => {
        #[tokio::test]
        async fn $t() -> Result<()> {
            dotenvy::from_filename(".test.env")?;
            let client = util::connect_lnd(None).await?;
            lightning::$t(&client).await?;
            Ok(())
        }
    };
}

#[tokio::test]
async fn timeout() -> Result<()> {
    dotenvy::from_filename(".test.env")?;
    let client = util::connect_lnd(Some(Duration::from_nanos(1))).await?;
    lightning::timeout(&client).await?;
    Ok(())
}

test_method!(get_info);
test_method!(create_invoice);
