//! lnd v0.16.4-beta grpc api

use crate::{lightning::*, Result};
use hyper::{
    client::{HttpConnector, ResponseFuture},
    Body, Client, Request, Response, Uri,
};
use hyper_openssl::HttpsConnector;
use openssl::{
    ssl::{SslConnector, SslMethod},
    x509::X509,
};
use std::{path::Path, task::Poll};
use tokio::fs;
use tonic::{body::BoxBody, codegen::InterceptedService};
use tower::Service;

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

pub type MacaroonChannel = InterceptedService<LndChannel, MacaroonInterceptor>;

pub type LightningClient = lnrpc::lightning_client::LightningClient<MacaroonChannel>;
pub type PeersClient = peersrpc::peers_client::PeersClient<MacaroonChannel>;
pub type SignerClient = signrpc::signer_client::SignerClient<MacaroonChannel>;
pub type VersionerClient = verrpc::versioner_client::VersionerClient<MacaroonChannel>;
pub type WalletKitClient = walletrpc::wallet_kit_client::WalletKitClient<MacaroonChannel>;

#[derive(Clone, Debug)]
pub struct Lnd {
    lightning: LightningClient,
    wallet: WalletKitClient,
    signer: SignerClient,
    peers: PeersClient,
    version: VersionerClient,
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

    pub async fn connect<CF, MF>(url: String, cert_file: CF, macaroon_file: MF) -> Result<Self>
    where
        CF: AsRef<Path>,
        MF: AsRef<Path>,
    {
        let cert = fs::read(cert_file.as_ref()).await?;
        let macaroon = fs::read(macaroon_file.as_ref()).await?;
        Self::connect2(url, cert, macaroon).await
    }

    pub async fn connect2<CF, MF>(url: String, cert: CF, macaroon: MF) -> Result<Self>
    where
        CF: AsRef<[u8]>,
        MF: AsRef<[u8]>,
    {
        let uri: Uri = url.parse()?;
        let interceptor = MacaroonInterceptor {
            macaroon: hex::encode(macaroon.as_ref()),
        };
        let channel = LndChannel::new(cert.as_ref(), uri).await?;

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
            signer: signrpc::signer_client::SignerClient::with_interceptor(channel, interceptor),
        })
    }
}

type TlsClient = Client<HttpsConnector<HttpConnector>, BoxBody>;
const ALPN_H2_WIRE: &[u8] = b"\x02h2";

#[derive(Clone, Debug)]
pub struct LndChannel {
    uri: Uri,
    client: TlsClient,
}

impl LndChannel {
    pub async fn new(pem: &[u8], uri: Uri) -> Result<Self> {
        let mut http = HttpConnector::new();
        http.enforce_http(false);

        let ca = X509::from_pem(pem)?;
        let mut connector = SslConnector::builder(SslMethod::tls())?;
        connector.cert_store_mut().add_cert(ca)?;
        connector.set_alpn_protos(ALPN_H2_WIRE)?;
        let mut https = HttpsConnector::with_connector(http, connector)?;
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
        })
    }
}
