use crate::{api, lndhub, lnurl, nip05, nwc::Nwc, setting::Setting, Error, Result, Service};
use actix_cors::Cors;
use actix_web::{
    body::MessageBody,
    dev::{ServiceFactory, ServiceRequest},
    middleware, web, App as WebApp, HttpServer,
};
use lightning_client::{Cln, Lightning, Lnd};
use sea_orm::{ConnectOptions, Database};
use std::{path::Path, sync::Arc, time::Duration};
use tracing::info;

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

        info!("{:?}", setting);

        Self::from_setting(setting).await
    }

    pub async fn from_setting(setting: Setting) -> Result<Self> {
        let timeout = Some(Duration::from_secs(5));
        let conf: (String, Box<dyn Lightning + Sync + Send>) = match setting.lightning {
            crate::setting::Lightning::Lnd => {
                let s = setting
                    .lnd
                    .clone()
                    .ok_or_else(|| Error::Message("Need config lnd".to_string()))?;
                let lightning = Lnd::connect(s.url, s.cert, s.macaroon, timeout).await?;
                ("lnd".to_owned(), Box::new(lightning))
            }
            crate::setting::Lightning::Cln => {
                let s = setting
                    .cln
                    .clone()
                    .ok_or_else(|| Error::Message("Need config cln".to_string()))?;
                let lightning = Cln::connect(s.url, s.ca, s.client, s.client_key, timeout).await?;
                ("cln".to_owned(), Box::new(lightning))
            }
        };

        let mut options = ConnectOptions::from(&setting.db_url);
        options.sqlx_logging_level(tracing::log::LevelFilter::Trace);
        let conn = Database::connect(options).await?;
        let service = Service::new(conf.0, conf.1, conn);

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
        .configure(lndhub::configure)
        .service(api::scope())
        .service(
            web::scope("/.well-known")
                .wrap(
                    Cors::default()
                        .send_wildcard()
                        .allow_any_header()
                        .allow_any_origin()
                        .allow_any_method()
                        .max_age(86_400),
                )
                .service(lnurl::scope())
                .service(nip05::info),
        )
}

/// start the service sync task for sync invoices and payments from lightning node.
pub fn start_service_sync(state: Arc<AppState>) {
    let _r = tokio::spawn(async move {
        state
            .service
            .sync(Duration::from_secs(5), Duration::from_secs(60 * 60 * 25))
            .await
    });
}

/// start nwc task
pub async fn start_nwc(state: Arc<AppState>) -> Result<()> {
    let nwc = Nwc::new(state);
    nwc.connect().await?;
    tokio::spawn(async move { nwc.handle_notifications().await });
    Ok(())
}

/// start app and tasks
pub async fn start(state: AppState) -> Result<()> {
    let state = web::Data::new(state);

    start_service_sync(state.clone().into_inner());
    if state.setting.nwc.support() {
        info!("Start nwc");
        start_nwc(state.clone().into_inner()).await?;
    } else {
        info!("nwc disabled");
    }

    let c_data = state.clone();
    let server = HttpServer::new(move || create_web_app(c_data.clone()));
    let num = if state.setting.thread.http == 0 {
        num_cpus::get()
    } else {
        state.setting.thread.http
    };
    let host = state.setting.network.host.clone();
    let port = state.setting.network.port;
    info!("Start http server {}:{}", host, port);
    server.workers(num).bind((host, port))?.run().await?;
    Ok(())
}
