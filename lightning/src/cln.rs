//! cln v23.05.2 grpc api

use crate::{lightning::*, Result};
use std::path::Path;
use tokio::fs;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

pub mod cln {
    tonic::include_proto!("cln");
}
use cln::{node_client::NodeClient, *};

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

#[tonic::async_trait]
impl Lightning for Cln {
    async fn get_info(&mut self) -> Result<Info> {
        let info = self.node.getinfo(GetinfoRequest {}).await?.into_inner();

        Ok(Info { id: info.id })
    }
}
