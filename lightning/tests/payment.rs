mod lightning;
use anyhow::Result;
mod util;

macro_rules! test_method {
    ($t:ident, $c1:ident, $c2: ident) => {
        #[tokio::test]
        async fn $t() -> Result<()> {
            dotenvy::dotenv()?;
            let c1 = util::$c1(None).await?;
            let c2 = util::$c2(None).await?;
            lightning::$t(&c1, &c2).await?;
            Ok(())
        }
    };
}

mod cln_to_lnd {
    use super::*;
    test_method!(payment, connect_cln, connect_lnd);
    test_method!(payment_error, connect_cln, connect_lnd);
}

mod lnd_to_cln {
    use super::*;
    test_method!(payment, connect_lnd, connect_cln);
    test_method!(payment_error, connect_lnd, connect_cln);
}
