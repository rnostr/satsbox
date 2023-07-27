//! lnd v0.16.4-beta grpc api

use crate::{lightning::*, Error, Result};
use hyper::{
    client::{HttpConnector, ResponseFuture},
    Body, Client, Request, Response, Uri,
};
use hyper_openssl::HttpsConnector;
use openssl::{
    ssl::{SslConnector, SslMethod},
    x509::X509,
};
use std::{path::Path, task::Poll, time::Duration};
use tokio::fs;
use tonic::{body::BoxBody, codegen::InterceptedService, Code};
use tower::{timeout::TimeoutLayer, Service, ServiceBuilder};

pub mod lnrpc {
    #![allow(clippy::all)]
    tonic::include_proto!("lnrpc");
}

pub mod walletrpc {
    #![allow(clippy::all)]
    tonic::include_proto!("walletrpc");
}

pub mod signrpc {
    #![allow(clippy::all)]
    tonic::include_proto!("signrpc");
}

pub mod verrpc {
    #![allow(clippy::all)]
    tonic::include_proto!("verrpc");
}

pub mod peersrpc {
    #![allow(clippy::all)]
    tonic::include_proto!("peersrpc");
}

pub mod routerrpc {
    #![allow(clippy::all)]
    tonic::include_proto!("routerrpc");
}

pub type MacaroonChannel = InterceptedService<LndChannel, MacaroonInterceptor>;

pub type LightningClient = lnrpc::lightning_client::LightningClient<MacaroonChannel>;
pub type PeersClient = peersrpc::peers_client::PeersClient<MacaroonChannel>;
pub type SignerClient = signrpc::signer_client::SignerClient<MacaroonChannel>;
pub type VersionerClient = verrpc::versioner_client::VersionerClient<MacaroonChannel>;
pub type WalletKitClient = walletrpc::wallet_kit_client::WalletKitClient<MacaroonChannel>;
pub type RouterClient = routerrpc::router_client::RouterClient<MacaroonChannel>;

#[derive(Clone, Debug)]
pub struct Lnd {
    lightning: LightningClient,
    wallet: WalletKitClient,
    signer: SignerClient,
    peers: PeersClient,
    version: VersionerClient,
    router: RouterClient,
}

impl Lnd {
    /// Returns the lightning client.
    pub fn lightning(&mut self) -> &mut LightningClient {
        &mut self.lightning
    }

    /// Returns the wallet client.
    pub fn wallet(&mut self) -> &mut WalletKitClient {
        &mut self.wallet
    }

    /// Returns the signer client.
    pub fn signer(&mut self) -> &mut SignerClient {
        &mut self.signer
    }

    /// Returns the versioner client.
    pub fn versioner(&mut self) -> &mut VersionerClient {
        &mut self.version
    }

    /// Returns the peers client.
    pub fn peers(&mut self) -> &mut PeersClient {
        &mut self.peers
    }

    /// Returns the router client.
    pub fn router(&mut self) -> &mut RouterClient {
        &mut self.router
    }

    pub async fn connect<CF, MF>(
        url: String,
        cert_file: CF,
        macaroon_file: MF,
        timeout: Option<Duration>,
    ) -> Result<Self>
    where
        CF: AsRef<Path>,
        MF: AsRef<Path>,
    {
        let cert = fs::read(cert_file.as_ref()).await?;
        let macaroon = fs::read(macaroon_file.as_ref()).await?;
        Self::connect2(url, cert, macaroon, timeout).await
    }

    pub async fn connect2<CF, MF>(
        url: String,
        cert: CF,
        macaroon: MF,
        timeout: Option<Duration>,
    ) -> Result<Self>
    where
        CF: AsRef<[u8]>,
        MF: AsRef<[u8]>,
    {
        let uri: Uri = url.parse()?;
        let interceptor = MacaroonInterceptor {
            macaroon: hex::encode(macaroon.as_ref()),
        };
        let channel = LndChannel::new(cert.as_ref(), uri, timeout).await?;

        Ok(Self {
            lightning: lnrpc::lightning_client::LightningClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            wallet: walletrpc::wallet_kit_client::WalletKitClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            peers: peersrpc::peers_client::PeersClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            version: verrpc::versioner_client::VersionerClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            signer: signrpc::signer_client::SignerClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            router: routerrpc::router_client::RouterClient::with_interceptor(channel, interceptor),
        })
    }
}

type TlsClient = Client<
    HttpsConnector<tower::util::Either<tower::timeout::Timeout<HttpConnector>, HttpConnector>>,
    BoxBody,
>;
const ALPN_H2_WIRE: &[u8] = b"\x02h2";

#[derive(Clone, Debug)]
pub struct LndChannel {
    uri: Uri,
    client: TlsClient,
}

impl LndChannel {
    pub async fn new(pem: &[u8], uri: Uri, timeout: Option<Duration>) -> Result<Self> {
        let mut http = HttpConnector::new();
        http.enforce_http(false);

        let connector = ServiceBuilder::new()
            .option_layer(timeout.map(TimeoutLayer::new))
            .service(http);

        let ca = X509::from_pem(pem)?;
        let mut ssl = SslConnector::builder(SslMethod::tls())?;
        ssl.cert_store_mut().add_cert(ca)?;
        ssl.set_alpn_protos(ALPN_H2_WIRE)?;
        let mut https = HttpsConnector::with_connector(connector, ssl)?;
        https.set_callback(|c, _| {
            c.set_verify_hostname(false);
            Ok(())
        });
        let client = Client::builder().http2_only(true).build(https);

        Ok(Self { client, uri })
    }
}

impl Service<Request<BoxBody>> for LndChannel {
    type Response = Response<Body>;
    type Error = hyper::Error;
    type Future = ResponseFuture;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, mut req: Request<BoxBody>) -> Self::Future {
        let uri = Uri::builder()
            .scheme(self.uri.scheme().unwrap().clone())
            .authority(self.uri.authority().unwrap().clone())
            .path_and_query(req.uri().path_and_query().unwrap().clone())
            .build()
            .unwrap();
        *req.uri_mut() = uri;
        self.client.request(req)
    }
}

/// Supplies requests with macaroon
#[derive(Clone)]
pub struct MacaroonInterceptor {
    macaroon: String,
}

impl tonic::service::Interceptor for MacaroonInterceptor {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        request.metadata_mut().insert(
            "macaroon",
            tonic::metadata::MetadataValue::try_from(&self.macaroon)
                .expect("hex produced non-ascii"),
        );
        Ok(request)
    }
}

#[tonic::async_trait]
impl Lightning for Lnd {
    async fn get_info(&self) -> Result<Info> {
        let info = self
            .lightning
            .clone()
            .get_info(lnrpc::GetInfoRequest {})
            .await?
            .into_inner();

        Ok(Info {
            id: hex::decode(info.identity_pubkey)?,
            alias: info.alias,
            color: info.color,
        })
    }

    async fn create_invoice(
        &self,
        memo: String,
        msats: u64,
        preimage: Option<Vec<u8>>,
        expiry: Option<u64>,
    ) -> Result<Invoice> {
        let data = self
            .lightning
            .clone()
            .add_invoice(lnrpc::Invoice {
                memo,
                r_preimage: preimage.unwrap_or_default(),
                value_msat: msats as i64,
                expiry: expiry.unwrap_or_default() as i64,
                ..Default::default()
            })
            .await?
            .into_inner();

        let mut invoice = Invoice::from_bolt11(data.payment_request)?;
        invoice.id = data.add_index.to_string();
        invoice.status = InvoiceStatus::Open;
        Ok(invoice)

        // let id = data.add_index.to_string();
        // let bolt11 = data.payment_request;

        // let data = self
        //     .lightning
        //     .clone()
        //     .decode_pay_req(lnrpc::PayReqString {
        //         pay_req: bolt11.clone(),
        //     })
        //     .await?
        //     .into_inner();
        // Ok(Invoice {
        //     id,
        //     bolt11,
        //     payee: hex::decode(data.destination)?,
        //     payment_hash: hex::decode(data.payment_hash)?,
        //     payment_secret: data.payment_addr,
        //     description: data.description,
        //     amount: data.num_msat as u64,
        //     expiry: data.expiry as u64,
        //     created_at: data.timestamp as u64,
        //     cltv_expiry: data.cltv_expiry as u64,
        // })
    }

    async fn lookup_invoice(&self, payment_hash: Vec<u8>) -> Result<Invoice> {
        let data = self
            .lightning
            .clone()
            .lookup_invoice(lnrpc::PaymentHash {
                r_hash: payment_hash,
                ..Default::default()
            })
            .await
            .map_err(|err| {
                if err.code() == Code::NotFound {
                    Error::InvoiceNotFound
                } else {
                    err.into()
                }
            })?
            .into_inner();

        map_invoice(data)
    }

    async fn list_invoices(&self, from: Option<u64>, to: Option<u64>) -> Result<Vec<Invoice>> {
        let data = self
            .lightning
            .clone()
            .list_invoices(lnrpc::ListInvoiceRequest {
                creation_date_start: from.unwrap_or_default(),
                creation_date_end: to.unwrap_or_default(),
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

    async fn pay(&self, bolt11: String) -> Result<Vec<u8>> {
        let data = self
            .lightning
            .clone()
            .send_payment_sync(lnrpc::SendRequest {
                payment_request: bolt11,
                ..Default::default()
            })
            .await?
            .into_inner();

        // println!("pay {:?}", data);
        // SendResponse { payment_error: "", payment_preimage: [220,
        // pay SendResponse { payment_error: "insufficient_balance", payment_preimage: []
        if data.payment_preimage.is_empty() {
            return Err(Error::Message(data.payment_error));
        }
        Ok(data.payment_hash)
    }

    async fn lookup_payment(&self, payment_hash: Vec<u8>) -> Result<Payment> {
        let mut stream = self
            .router
            .clone()
            .track_payment_v2(routerrpc::TrackPaymentRequest {
                payment_hash: payment_hash.clone(),
                no_inflight_updates: true,
            })
            .await
            .map_err(|err| {
                if err.code() == Code::NotFound {
                    Error::PaymentNotFound
                } else {
                    err.into()
                }
            })?
            .into_inner();

        let msg = tokio::time::timeout(Duration::from_secs(2), stream.message()).await;
        if let Ok(msg) = msg {
            let msg = msg?;
            if let Some(payment) = msg {
                map_payment(payment)
            } else {
                Err(Error::Message("missing payment".to_owned()))
            }
        } else {
            // timeout, make payment in flight
            return Ok(Payment {
                payment_hash,
                status: PaymentStatus::InFlight,
                ..Default::default()
            });
        }
    }

    async fn list_payments(&self, from: Option<u64>, to: Option<u64>) -> Result<Vec<Payment>> {
        let data = self
            .lightning
            .clone()
            .list_payments(lnrpc::ListPaymentsRequest {
                creation_date_start: from.unwrap_or_default(),
                creation_date_end: to.unwrap_or_default(),
                ..Default::default()
            })
            .await?
            .into_inner();

        let mut list = vec![];
        for item in data.payments {
            list.push(map_payment(item)?);
        }
        Ok(list)
    }
}

fn map_invoice(data: lnrpc::Invoice) -> Result<Invoice> {
    let status = match data.state() {
        lnrpc::invoice::InvoiceState::Canceled => InvoiceStatus::Canceled,
        lnrpc::invoice::InvoiceState::Settled => InvoiceStatus::Paid,
        _ => InvoiceStatus::Open,
    };

    let mut invoice = Invoice::from_bolt11(data.payment_request)?;
    invoice.id = data.add_index.to_string();
    invoice.status = status;
    invoice.paid_at = data.settle_date as u64;
    invoice.paid_amount = data.amt_paid_msat as u64;
    Ok(invoice)
}

fn map_payment(payment: lnrpc::Payment) -> Result<Payment> {
    let status = match payment.status() {
        lnrpc::payment::PaymentStatus::Unknown => PaymentStatus::Unknown,
        lnrpc::payment::PaymentStatus::InFlight => PaymentStatus::InFlight,
        lnrpc::payment::PaymentStatus::Succeeded => PaymentStatus::Succeeded,
        lnrpc::payment::PaymentStatus::Failed => PaymentStatus::Failed,
    };
    Ok(Payment {
        id: payment.payment_index.to_string(),
        bolt11: payment.payment_request,
        payment_hash: hex::decode(payment.payment_hash)?,
        payment_preimage: hex::decode(payment.payment_preimage)?,
        created_at: Duration::from_nanos(payment.creation_time_ns as u64).as_secs(),
        amount: payment.value_msat as u64,
        fee: payment.fee_msat as u64,
        total: (payment.value_msat + payment.fee_msat) as u64,
        status,
    })
}
