use std::path::Path;

use crate::{setting::Setting, Error, Result, Service};
use actix_web::{
    body::MessageBody,
    dev::{ServiceFactory, ServiceRequest},
    middleware, web, App as WebApp, HttpServer,
};
use lightning_client::{Cln, Lightning, Lnd};
use sea_orm::{ConnectOptions, Database};
use tracing::info;

pub mod route {
    use super::*;
    use actix_web::{get, HttpResponse};

    pub fn init(cfg: &mut web::ServiceConfig) {
        cfg.service(info);
    }

    #[get("/info")]
    pub async fn info(data: web::Data<AppState>) -> Result<HttpResponse, Error> {
        let info = data.service.info().await?;
        Ok(HttpResponse::Ok().json(info))
    }
}

pub struct AppState {
    pub service: Service,
    pub setting: Setting,
}

impl AppState {
    pub async fn create<P: AsRef<Path>>(
        setting_path: Option<P>,
        setting_env_prefix: Option<String>,
    ) -> Result<Self> {
        let env_notice = setting_env_prefix
            .as_ref()
            .map(|s| {
                format!(
                    ", config will be overrided by ENV seting with prefix `{}_`",
                    s
                )
            })
            .unwrap_or_default();

        let setting = if let Some(path) = setting_path {
            info!("Load config {:?}{}", path.as_ref(), env_notice);
            Setting::read(path.as_ref(), setting_env_prefix)?
        } else if let Some(prefix) = setting_env_prefix {
            info!("Load default config{}", env_notice);
            Setting::from_env(prefix)?
        } else {
            info!("Load default config");
            Setting::default()
        };
        Self::from_setting(setting).await
    }

    pub async fn from_setting(setting: Setting) -> Result<Self> {
        let lightning: Box<dyn Lightning + Sync + Send> = match setting.lightning {
            crate::setting::Lightning::Lnd => {
                let s = setting
                    .lnd
                    .clone()
                    .ok_or_else(|| Error::Message("Need config lnd".to_string()))?;
                let lightning = Lnd::connect(s.url, s.cert, s.macaroon).await?;
                Box::new(lightning)
            }
            crate::setting::Lightning::Cln => {
                let s = setting
                    .cln
                    .clone()
                    .ok_or_else(|| Error::Message("Need config cln".to_string()))?;
                let lightning = Cln::connect(s.url, s.ca, s.client, s.client_key).await?;
                Box::new(lightning)
            }
        };

        let options = ConnectOptions::from(&setting.db_url);
        let conn = Database::connect(options).await?;
        let service = Service::new(lightning, conn);

        Ok(Self { service, setting })
    }
}

pub fn create_web_app(
    data: web::Data<AppState>,
) -> WebApp<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse<impl MessageBody>,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    WebApp::new()
        .app_data(data)
        .wrap(middleware::Logger::default()) // enable logger
        .configure(route::init)
}

pub async fn start(state: AppState) -> Result<()> {
    let data = web::Data::new(state);
    let c_data = data.clone();
    let server = HttpServer::new(move || create_web_app(c_data.clone()));
    let num = if data.setting.thread.http == 0 {
        num_cpus::get()
    } else {
        data.setting.thread.http
    };
    let host = data.setting.network.host.clone();
    let port = data.setting.network.port;
    info!("Start http server {}:{}", host, port);
    server.workers(num).bind((host, port))?.run().await?;
    Ok(())
}
