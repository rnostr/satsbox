//! cln v23.05.2 grpc api

use crate::{lightning::*, Error, Result};
use rand::RngCore;
use std::path::Path;
use tokio::fs;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

pub mod cln {
    #![allow(clippy::all)]
    tonic::include_proto!("cln");
}
use cln::{node_client::NodeClient, *};

#[derive(Clone, Debug)]
pub struct Cln {
    node: NodeClient<Channel>,
}

impl Cln {
    pub fn node(&mut self) -> &mut NodeClient<Channel> {
        &mut self.node
    }

    pub async fn connect<CF, TF, KF>(
        url: String,
        ca_file: CF,
        client_file: TF,
        client_key_file: KF,
    ) -> Result<Self>
    where
        CF: AsRef<Path>,
        TF: AsRef<Path>,
        KF: AsRef<Path>,
    {
        let ca = fs::read(ca_file.as_ref()).await?;
        let client_pem = fs::read(client_file.as_ref()).await?;
        let client_key = fs::read(client_key_file.as_ref()).await?;
        Self::connect2(url, ca, client_pem, client_key).await
    }

    pub async fn connect2<CF, TF, KF>(
        url: String,
        ca: CF,
        client_pem: TF,
        client_key: KF,
    ) -> Result<Self>
    where
        CF: AsRef<[u8]>,
        TF: AsRef<[u8]>,
        KF: AsRef<[u8]>,
    {
        let ca = Certificate::from_pem(&ca);
        let ident = Identity::from_pem(&client_pem, &client_key);

        let tls = ClientTlsConfig::new()
            .domain_name("cln")
            .identity(ident)
            .ca_certificate(ca);

        let channel = Channel::from_shared(url)?
            .tls_config(tls)?
            .connect()
            .await?;

        Ok(Self {
            node: NodeClient::new(channel),
        })
    }
}

fn amount_or_any(msat: u64) -> Option<AmountOrAny> {
    Some(AmountOrAny {
        value: Some(amount_or_any::Value::Amount(amount(msat))),
    })
}
// fn amount_or_all(msat: u64) -> Option<AmountOrAll> {
//     Some(AmountOrAll {
//         value: Some(amount_or_all::Value::Amount(amount(msat))),
//     })
// }

fn amount(msat: u64) -> Amount {
    Amount { msat }
}

fn rand_id() -> String {
    let mut store_key_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut store_key_bytes);
    hex::encode(store_key_bytes)
}

#[tonic::async_trait]
impl Lightning for Cln {
    async fn get_info(&self) -> Result<Info> {
        let info = self
            .node
            .clone()
            .getinfo(GetinfoRequest {})
            .await?
            .into_inner();

        Ok(Info {
            id: info.id,
            color: String::from_utf8(info.color).unwrap_or_default(),
            alias: info.alias.unwrap_or_default(),
        })
    }

    async fn create_invoice(
        &self,
        memo: String,
        msats: u64,
        preimage: Option<Vec<u8>>,
        expiry: Option<u64>,
    ) -> Result<Invoice> {
        let id = rand_id();
        let data = self
            .node
            .clone()
            .invoice(InvoiceRequest {
                amount_msat: amount_or_any(msats),
                preimage,
                description: memo,
                expiry,
                label: id.clone(),
                ..Default::default()
            })
            .await?
            .into_inner();

        // Ok(Invoice::from(id, data.bolt11)?)

        let bolt11 = data.bolt11;
        let data = self
            .node
            .clone()
            .decode_pay(DecodepayRequest {
                bolt11: bolt11.clone(),
                ..Default::default()
            })
            .await?
            .into_inner();

        Ok(Invoice {
            id,
            bolt11,
            payee: data.payee,
            payment_hash: data.payment_hash,
            payment_secret: data
                .payment_secret
                .ok_or_else(|| Error::Invalid("missing payee".to_owned()))?,
            description: data.description.unwrap_or_default(),
            amount: data
                .amount_msat
                .ok_or_else(|| Error::Invalid("missing amount".to_owned()))?
                .msat,
            expiry: data.expiry,
            created_at: data.created_at,
            cltv_expiry: data.min_final_cltv_expiry as u64,
        })
    }
}
