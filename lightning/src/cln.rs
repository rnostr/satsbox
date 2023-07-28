//! cln v23.05.2 grpc api

use crate::{lightning::*, Error, Result};
use rand::RngCore;
use std::{path::Path, time::Duration};
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
        timeout: Option<Duration>,
    ) -> Result<Self>
    where
        CF: AsRef<Path>,
        TF: AsRef<Path>,
        KF: AsRef<Path>,
    {
        let ca = fs::read(ca_file.as_ref()).await?;
        let client_pem = fs::read(client_file.as_ref()).await?;
        let client_key = fs::read(client_key_file.as_ref()).await?;
        Self::connect2(url, ca, client_pem, client_key, timeout).await
    }

    pub async fn connect2<CF, TF, KF>(
        url: String,
        ca: CF,
        client_pem: TF,
        client_key: KF,
        timeout: Option<Duration>,
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

        let mut endpoint = Channel::from_shared(url)?;
        if let Some(timeout) = timeout {
            endpoint = endpoint.timeout(timeout);
        }

        let channel = endpoint.tls_config(tls)?.connect().await?;

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

        let mut invoice = Invoice::from_bolt11(data.bolt11)?;
        invoice.id = id;
        invoice.status = InvoiceStatus::Open;
        Ok(invoice)

        // let bolt11 = data.bolt11;
        // let data = self
        //     .node
        //     .clone()
        //     .decode_pay(DecodepayRequest {
        //         bolt11: bolt11.clone(),
        //         ..Default::default()
        //     })
        //     .await?
        //     .into_inner();

        // Ok(Invoice {
        //     id,
        //     bolt11,
        //     payee: data.payee,
        //     payment_hash: data.payment_hash,
        //     payment_secret: data
        //         .payment_secret
        //         .ok_or_else(|| Error::Invalid("missing payee".to_owned()))?,
        //     description: data.description.unwrap_or_default(),
        //     amount: data
        //         .amount_msat
        //         .ok_or_else(|| Error::Invalid("missing amount".to_owned()))?
        //         .msat,
        //     expiry: data.expiry,
        //     created_at: data.created_at,
        //     cltv_expiry: data.min_final_cltv_expiry as u64,
        // })
    }

    async fn lookup_invoice(&self, payment_hash: Vec<u8>) -> Result<Invoice> {
        let data = self
            .node
            .clone()
            .list_invoices(ListinvoicesRequest {
                payment_hash: Some(payment_hash),
                ..Default::default()
            })
            .await?
            .into_inner();
        if data.invoices.is_empty() {
            return Err(Error::InvoiceNotFound);
        }
        map_invoice(data.invoices[0].clone())
    }

    // wait lnd support pagination
    // https://github.com/ElementsProject/lightning/issues/6348
    async fn list_invoices(&self, _from: Option<u64>, _to: Option<u64>) -> Result<Vec<Invoice>> {
        let data = self
            .node
            .clone()
            .list_invoices(ListinvoicesRequest {
                ..Default::default()
            })
            .await?
            .into_inner();
        let mut invoices = vec![];
        for invoice in data.invoices {
            invoices.push(map_invoice(invoice)?);
        }
        Ok(invoices)
    }

    async fn pay(&self, bolt11: String, max_fee_msat: Option<u64>) -> Result<Vec<u8>> {
        let data = self
            .node
            .clone()
            .pay(PayRequest {
                bolt11,
                maxfee: max_fee_msat.map(amount),
                ..Default::default()
            })
            .await?
            .into_inner();
        // println!("pay {:?}", data);
        // pay PayResponse { payment_preimage: [172, 241, 137, 129, 56, 79, 183, 54, 165, 125, 87, 193, 242, 114, 162, 235, 204, 238, 111, 92, 73, 235, 160, 106, 9, 7, 17, 27, 245, 94, 24, 236], destination: Some([2, 182, 98, 15, 108, 86, 15, 55, 45, 158, 162, 41, 235, 155, 198, 90, 96, 22, 138, 73, 14, 152, 5, 212, 238, 35, 202, 46, 91, 63, 247, 210, 91]), payment_hash: [67, 161, 103, 205, 182, 212, 83, 15, 204, 106, 68, 75, 62, 230, 156, 191, 29, 243, 143, 173, 1, 216, 111, 95, 24, 207, 157, 212, 16, 181, 9, 228], created_at: 1690340501.474, parts: 1, amount_msat: Some(Amount { msat: 100000 }), amount_sent_msat: Some(Amount { msat: 100000 }), warning_partial_completion: None, status: Complete }
        Ok(data.payment_hash)
    }

    async fn lookup_payment(&self, payment_hash: Vec<u8>) -> Result<Payment> {
        let data = self
            .node
            .clone()
            .list_send_pays(ListsendpaysRequest {
                payment_hash: Some(payment_hash),
                ..Default::default()
            })
            .await?
            .into_inner();
        if data.payments.is_empty() {
            return Err(Error::PaymentNotFound);
        }
        Ok(map_payment(data.payments[0].clone()))
    }

    // wait lnd support pagination
    // https://github.com/ElementsProject/lightning/issues/6348
    async fn list_payments(&self, _from: Option<u64>, _to: Option<u64>) -> Result<Vec<Payment>> {
        let data = self
            .node
            .clone()
            .list_send_pays(ListsendpaysRequest {
                ..Default::default()
            })
            .await?
            .into_inner();
        Ok(data.payments.into_iter().map(map_payment).collect())
    }
}

fn map_invoice(inv: ListinvoicesInvoices) -> Result<Invoice> {
    let status = match inv.status() {
        listinvoices_invoices::ListinvoicesInvoicesStatus::Unpaid => InvoiceStatus::Open,
        listinvoices_invoices::ListinvoicesInvoicesStatus::Paid => InvoiceStatus::Paid,
        listinvoices_invoices::ListinvoicesInvoicesStatus::Expired => InvoiceStatus::Canceled,
    };

    let mut invoice = Invoice::from_bolt11(
        inv.bolt11
            .ok_or_else(|| Error::Invalid("missing bolt11".to_owned()))?,
    )?;
    invoice.id = inv.label;
    invoice.status = status;
    invoice.paid_at = inv.paid_at.unwrap_or_default();
    invoice.paid_amount = inv.amount_received_msat.map(|m| m.msat).unwrap_or_default();
    Ok(invoice)
}

fn map_payment(payment: ListsendpaysPayments) -> Payment {
    let status = match payment.status() {
        listsendpays_payments::ListsendpaysPaymentsStatus::Complete => PaymentStatus::Succeeded,
        listsendpays_payments::ListsendpaysPaymentsStatus::Pending => PaymentStatus::InFlight,
        listsendpays_payments::ListsendpaysPaymentsStatus::Failed => PaymentStatus::Failed,
    };

    let amount = payment.amount_msat.map(|m| m.msat).unwrap_or_default();
    let total = payment.amount_sent_msat.map(|m| m.msat).unwrap_or_default();
    Payment {
        status,
        id: payment.id.to_string(),
        bolt11: payment.bolt11.unwrap_or_default(),
        payment_hash: payment.payment_hash,
        payment_preimage: payment.payment_preimage.unwrap_or_default(),
        created_at: payment.created_at,
        amount,
        fee: total - amount,
        total,
    }
}
