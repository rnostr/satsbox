use crate::{Error, Result};
use dyn_clone::DynClone;
use lightning_invoice::SignedRawBolt11Invoice;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    #[serde(with = "hex::serde")]
    pub id: Vec<u8>,
    pub alias: String,
    pub color: String,
    pub num_peers: u32,
    pub num_pending_channels: u32,
    pub num_active_channels: u32,
    pub num_inactive_channels: u32,
    pub version: String,
    pub block_height: u32,
}

#[derive(Copy, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum InvoiceStatus {
    Open = 0, // unpaid
    Paid = 1,
    Canceled = 2,
}

impl Default for InvoiceStatus {
    fn default() -> Self {
        Self::Open
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Invoice {
    pub id: String,
    pub bolt11: String,
    #[serde(with = "hex::serde")]
    pub payee: Vec<u8>,
    #[serde(with = "hex::serde")]
    pub payment_hash: Vec<u8>,
    #[serde(with = "hex::serde")]
    pub payment_secret: Vec<u8>,
    pub description: String,
    pub expiry: u64,
    /// amount in msats
    pub amount: u64,
    /// timestamp as a duration since the Unix epoch
    pub created_at: u64,
    pub cltv_expiry: u64,
    pub status: InvoiceStatus,
    /// When this invoice was paid. Measured in seconds since the unix epoch.
    pub paid_at: u64,
    /// the amount actually received (could be slightly greater than amount, since clients may overpay), in msats
    pub paid_amount: u64,
}

impl Invoice {
    pub fn from_bolt11(bolt11: String) -> Result<Self> {
        let inv = bolt11.parse::<SignedRawBolt11Invoice>()?;
        let payee = if let Some(key) = inv.payee_pub_key() {
            key.serialize().to_vec()
        } else {
            inv.recover_payee_pub_key()
                .map_err(Error::from)?
                .serialize()
                .to_vec()
        };
        Ok(Self {
            bolt11,
            payee,
            payment_hash: inv
                .payment_hash()
                .ok_or_else(|| Error::Invalid("missing payment_hash".to_owned()))?
                .0
                .to_vec(),
            payment_secret: inv
                .payment_secret()
                .ok_or_else(|| Error::Invalid("missing payment_secret".to_owned()))?
                .0
                .to_vec(),
            // https://github.com/lightning/bolts/blob/master/11-payment-encoding.md
            // Default is 3600 (1 hour) if not specified.
            expiry: inv
                .expiry_time()
                .map(|e| e.as_seconds())
                .unwrap_or(lightning_invoice::DEFAULT_EXPIRY_TIME),
            amount: inv
                .amount_pico_btc()
                .map(|v| v / 10)
                .ok_or_else(|| Error::Invalid("missing amount".to_owned()))?,
            created_at: inv.raw_invoice().data.timestamp.as_unix_timestamp(),
            cltv_expiry: inv
                .min_final_cltv_expiry_delta()
                .map(|d| d.0)
                .unwrap_or(lightning_invoice::DEFAULT_MIN_FINAL_CLTV_EXPIRY_DELTA), //Default is 18 if not specified.
            description: inv
                .description()
                .map(|d| d.clone().into_inner())
                .unwrap_or_default(),
            ..Default::default()
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Payment {
    pub id: String,
    pub bolt11: String,
    #[serde(with = "hex::serde")]
    pub payment_preimage: Vec<u8>,
    #[serde(with = "hex::serde")]
    pub payment_hash: Vec<u8>,
    /// amount in msats
    pub amount: u64,
    /// fee in msats
    pub fee: u64,
    /// total send in msats
    pub total: u64,
    /// timestamp as a duration since the Unix epoch
    pub created_at: u64,
    pub status: PaymentStatus,
}

#[derive(Copy, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum PaymentStatus {
    Unknown = 0,
    InFlight = 1,
    Succeeded = 2,
    Failed = 3,
}

impl Default for PaymentStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// the lightning trait for multiple backends
#[tonic::async_trait]
pub trait Lightning: DynClone {
    /// get lightning node info
    async fn get_info(&self) -> Result<Info>;
    /// create an invoice
    async fn create_invoice(
        &self,
        memo: String,
        msats: u64,
        preimage: Option<Vec<u8>>,
        expiry: Option<u64>,
    ) -> Result<Invoice>;

    /// lookup invoice
    async fn lookup_invoice(&self, payment_hash: Vec<u8>) -> Result<Invoice>;

    /// list invoices by creation time
    async fn list_invoices(&self, from: Option<u64>, to: Option<u64>) -> Result<Vec<Invoice>>;

    /// pay a lightning invoice, return payment hash,
    /// need check payment status by `lookup_payment` if error
    /// pay faild if lookup payment [`Error::PaymentNotFound`]
    async fn pay(&self, bolt11: String, max_fee_msat: Option<u64>) -> Result<Vec<u8>>;

    /// lookup payment, The data is unreliable until completion (successed or failed).
    async fn lookup_payment(&self, payment_hash: Vec<u8>) -> Result<Payment>;

    /// list payments by creation time
    async fn list_payments(&self, from: Option<u64>, to: Option<u64>) -> Result<Vec<Payment>>;
}

dyn_clone::clone_trait_object!(Lightning);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoice() -> Result<()> {
        // {
        //     "currency": "bcrt",
        //     "created_at": 1690033134,
        //     "expiry": 86400,
        //     "payee": "02b6620f6c560f372d9ea229eb9bc65a60168a490e9805d4ee23ca2e5b3ff7d25b",
        //     "amount_msat": 10000,
        //     "description": "",
        //     "min_final_cltv_expiry": 80,
        //     "payment_secret": "270af0dc83c4e8c269edfaccb972c2033108890b17f9861b2295dcde81743390",
        //     "features": "024200",
        //     "payment_hash": "491c7660a05278a1b1433088f57a53d8775d8d12799cb1ccd0b154f3e3e8d6aa",
        //     "signature": "3045022100cef2a6b2965e9c6c818827e9ebb8561d4669bc3bfb0c04d8c176d86b8aecdf5502203d4c6fd67727c623a5a264b4e1beebadfecd23c63180c116f5507677247ae213"
        //  }
        let inv = Invoice::from_bolt11("lnbcrt100n1pjthklwpp5fyw8vc9q2fu2rv2rxzy027jnmpm4mrgj0xwtrnxsk9208clg664qdqqcqzzsxqyz5vqsp5yu90phyrcn5vy60dltxtjukzqvcs3zgtzlucvxezjhwdaqt5xwgq9qyyssqeme2dv5kt6wxeqvgyl57hwzkr4rxn0pmlvxqfkxpwmvxhzhvma2n6nr06emj033r5k3xfd8phm46mlkdy0rrrqxpzm64qanhy3awyycpw4rz5g".to_owned())?;
        assert_eq!(inv.created_at, 1690033134);
        assert_eq!(inv.expiry, 86400);
        assert_eq!(
            inv.payee,
            hex::decode("02b6620f6c560f372d9ea229eb9bc65a60168a490e9805d4ee23ca2e5b3ff7d25b")?
        );
        assert_eq!(inv.amount, 10000);
        assert_eq!(inv.description, "");
        assert_eq!(inv.cltv_expiry, 80);
        assert_eq!(
            inv.payment_hash,
            hex::decode("491c7660a05278a1b1433088f57a53d8775d8d12799cb1ccd0b154f3e3e8d6aa")?
        );
        assert_eq!(
            inv.payment_secret,
            hex::decode("270af0dc83c4e8c269edfaccb972c2033108890b17f9861b2295dcde81743390")?
        );
        Ok(())
    }
}
