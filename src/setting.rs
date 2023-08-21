use crate::Error;
use crate::{hash::NoOpHasherDefault, Result};
use config::{Config, Environment, File, FileFormat};
use nostr_sdk::secp256k1::SecretKey;
use notify::{event::ModifyKind, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::num::NonZeroU32;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::{error, info};

pub const CARGO_PKG_VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

// fn default_version() -> String {
//     CARGO_PKG_VERSION.map(ToOwned::to_owned).unwrap_or_default()
// }

/// number of threads config
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Thread {
    /// number of http server threads
    pub http: usize,
}

/// network config
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Network {
    /// server bind host
    pub host: String,
    /// server bind port
    pub port: u16,

    pub real_ip_header: Option<String>,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            real_ip_header: None,
        }
    }
}

/// lightning client type
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lightning {
    Lnd,
    Cln,
}

impl Default for Lightning {
    fn default() -> Self {
        Self::Lnd
    }
}

/// Lnd setting
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
// #[serde(default)]
pub struct Lnd {
    /// lnd grpc url
    pub url: String,
    /// tls.cert
    pub cert: PathBuf,
    /// admin.macaroon
    pub macaroon: PathBuf,
}

/// Cln setting
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
// #[serde(default)]
pub struct Cln {
    /// cln grpc url
    pub url: String,
    /// ca.pem path
    pub ca: PathBuf,
    /// client.pem path
    pub client: PathBuf,
    /// client-key.pem
    pub client_key: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Fee {
    /// lightning: The fee limit expressed as a percentage of the payment amount. (0-100)
    pub pay_limit_pct: f32,
    /// lightning: small amounts (<=1k sat) payment max fee.
    pub small_pay_limit_pct: f32,
    /// internal pyament fee
    pub internal_pct: f32,
    /// service fee per payment
    pub service_pct: f32,
}

impl Default for Fee {
    fn default() -> Self {
        Self {
            pay_limit_pct: 2.0,
            small_pay_limit_pct: 10.0,
            internal_pct: 0.3,
            service_pct: 0.0,
        }
    }
}

fn pct(amount: i64, pct: f32) -> i64 {
    (amount as f64 * pct as f64 / 100.0).floor() as i64
}

impl Fee {
    pub fn cal(&self, msats: i64, internal: bool) -> (i64, i64) {
        let fee_pct = if internal {
            self.internal_pct
        } else if msats > 1_000_000 {
            self.pay_limit_pct
        } else {
            self.small_pay_limit_pct
        };
        (pct(msats, fee_pct), pct(msats, self.service_pct))
    }
}

/// auth config
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Auth {
    /// auth secret
    pub secret: String,

    /// jwt refresh token expiry in seconds
    pub refresh_token_expiry: usize,

    /// jwt access token expiry in seconds
    pub access_token_expiry: usize,
}

impl Default for Auth {
    fn default() -> Self {
        Self {
            secret: "test".to_owned(),
            refresh_token_expiry: 7 * 24 * 60 * 60,
            access_token_expiry: 2 * 24 * 60 * 60,
        }
    }
}

/// nwc config
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Nwc {
    /// relay server, don't support nwc if relays is empty.
    pub relays: Vec<String>,
    /// nwc private key, don't support nwc if not set.
    pub privkey: Option<SecretKey>,

    pub proxy: Option<String>,

    pub rate_limit_per_second: NonZeroU32,
}

impl Nwc {
    pub fn support(&self) -> bool {
        !self.relays.is_empty() && self.privkey.is_some()
    }
}

impl Default for Nwc {
    fn default() -> Self {
        Self {
            relays: vec![],
            privkey: None,
            proxy: None,
            rate_limit_per_second: NonZeroU32::new(10).unwrap(),
        }
    }
}

/// lnurl config
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Lnurl {
    pub min_sendable: u64,
    pub max_sendable: u64,
    pub comment_allowed: usize,
    /// nostr private key for send zap receipt, don't support nostr if not set.
    pub privkey: Option<SecretKey>,

    /// extra nostr relay server for sending zap receipt
    pub relays: Vec<String>,
    /// relay proxy
    pub proxy: Option<String>,
}

impl Default for Lnurl {
    fn default() -> Self {
        Self {
            min_sendable: 1_000,
            max_sendable: 10_000_000_000,
            comment_allowed: 255,
            privkey: None,
            relays: vec![],
            proxy: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Setting {
    /// database url
    /// https://www.sea-ql.org/SeaORM/docs/install-and-config/connection/
    pub db_url: String,

    /// the site url
    pub site: Option<String>,

    pub fee: Fee,

    pub thread: Thread,
    pub network: Network,

    pub lightning: Lightning,
    pub lightning_node: String,

    pub cln: Option<Cln>,
    pub lnd: Option<Lnd>,

    pub auth: Auth,
    pub nwc: Nwc,
    pub lnurl: Lnurl,

    /// flatten extensions setting to json::Value
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,

    /// extensions setting object
    #[serde(skip)]
    extensions: HashMap<TypeId, Box<dyn Any + Send + Sync>, NoOpHasherDefault>,
}

// impl Setting {
//     pub fn site(&self) -> String {
//         self.site
//             .clone()
//             .unwrap_or_else(|| format!("http://{}:{}", self.network.host, self.network.port))
//     }
// }

impl Default for Setting {
    fn default() -> Self {
        Self {
            db_url: "sqlite://satsbox.sqlite".to_string(),
            cln: None,
            lnd: None,
            site: None,
            lightning_node: "127.0.0.1:9735".to_string(),
            lightning: Default::default(),
            thread: Default::default(),
            network: Default::default(),
            fee: Default::default(),
            extra: Default::default(),
            extensions: Default::default(),
            auth: Default::default(),
            nwc: Default::default(),
            lnurl: Default::default(),
        }
    }
}

impl PartialEq for Setting {
    fn eq(&self, other: &Self) -> bool {
        self.db_url == other.db_url
            && self.thread == other.thread
            && self.network == other.network
            && self.extra == other.extra
    }
}

#[derive(Debug, Clone)]
pub struct SettingWrapper {
    inner: Arc<RwLock<Setting>>,
    watcher: Option<Arc<RecommendedWatcher>>,
}

impl Deref for SettingWrapper {
    type Target = Arc<RwLock<Setting>>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl From<Setting> for SettingWrapper {
    fn from(setting: Setting) -> Self {
        Self {
            inner: Arc::new(RwLock::new(setting)),
            watcher: None,
        }
    }
}

impl SettingWrapper {
    /// reload setting from file
    pub fn reload<P: AsRef<Path>>(&self, file: P, env_prefix: Option<String>) -> Result<()> {
        let setting = Setting::read(&file, env_prefix)?;
        {
            let mut w = self.write();
            *w = setting;
        }
        Ok(())
    }

    /// config from file and watch file update then reload
    pub fn watch<P: AsRef<Path>, F: Fn(&SettingWrapper) + Send + 'static>(
        file: P,
        env_prefix: Option<String>,
        f: F,
    ) -> Result<Self> {
        let mut setting: SettingWrapper = Setting::read(&file, env_prefix.clone())?.into();
        let c_setting = setting.clone();

        // let file = current_dir()?.join(file.as_ref());
        // symbolic links
        let file = fs::canonicalize(file.as_ref())?;
        let c_file = file.clone();

        // support vim editor. watch dir
        // https://docs.rs/notify/latest/notify/#editor-behaviour
        // https://github.com/notify-rs/notify/issues/113#issuecomment-281836995

        let dir = file
            .parent()
            .ok_or_else(|| Error::Message("failed to get config dir".to_owned()))?;

        let mut watcher = RecommendedWatcher::new(
            move |result: Result<Event, notify::Error>| match result {
                Ok(event) => {
                    // println!("event: {:?}", event);
                    if matches!(event.kind, EventKind::Modify(ModifyKind::Data(_)))
                        && event.paths.contains(&c_file)
                    {
                        match c_setting.reload(&c_file, env_prefix.clone()) {
                            Ok(_) => {
                                info!("Reload config success {:?}", c_file);
                                info!("{:?}", c_setting.read());
                                f(&c_setting);
                            }
                            Err(e) => {
                                error!(
                                    error = e.to_string(),
                                    "failed to reload config {:?}", c_file
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(error = e.to_string(), "failed to watch file {:?}", c_file);
                }
            },
            notify::Config::default(),
        )?;

        watcher.watch(dir, RecursiveMode::NonRecursive)?;
        // save watcher
        setting.watcher = Some(Arc::new(watcher));

        Ok(setting)
    }
}

impl Setting {
    /// get extension setting as json from extra
    pub fn get_extra_json(&self, key: &str) -> Option<String> {
        self.extra
            .get(key)
            .and_then(|h| serde_json::to_string(h).ok())
        // .map(|h| serde_json::to_string(h).ok())
        // .flatten()
    }

    /// Parse extension setting from extra json string.
    pub fn parse_extension<T: DeserializeOwned + Default>(&self, key: &str) -> T {
        self.get_extra_json(key)
            .and_then(|s| {
                let r = serde_json::from_str::<T>(&s);
                if let Err(err) = &r {
                    error!(error = err.to_string(), "failed to parse {:?} setting", key);
                }
                r.ok()
            })
            .unwrap_or_default()
    }

    /// save extension setting
    pub fn set_extension<T: Send + Sync + 'static>(&mut self, val: T) {
        self.extensions.insert(TypeId::of::<T>(), Box::new(val));
    }

    /// get extension setting
    pub fn get_extension<T: 'static>(&self) -> Option<&T> {
        self.extensions
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref())
    }

    /// read config from file and env
    pub fn read<P: AsRef<Path>>(file: P, env_prefix: Option<String>) -> Result<Self> {
        let builder = Config::builder();
        let mut config = builder
            // Use serde default feature, ignore the following code
            // // use defaults
            // .add_source(Config::try_from(&Self::default())?)
            // override with file contents
            .add_source(File::with_name(file.as_ref().to_str().unwrap()));
        if let Some(prefix) = env_prefix {
            config = config.add_source(Self::env_source(&prefix));
        }

        let config = config.build()?;
        let mut setting: Setting = config.try_deserialize()?;
        setting.validate()?;
        Ok(setting)
    }

    fn env_source(prefix: &str) -> Environment {
        Environment::with_prefix(prefix)
            .try_parsing(true)
            .prefix_separator("_")
            .separator("__")
            .list_separator(" ")
            .with_list_parse_key("nwc.relays")
            .with_list_parse_key("lnurl.relays")
    }

    /// read config from env
    pub fn from_env(env_prefix: String) -> Result<Self> {
        let mut config = Config::builder();
        config = config.add_source(Self::env_source(&env_prefix));

        let config = config.build()?;
        let mut setting: Setting = config.try_deserialize()?;
        setting.validate()?;
        Ok(setting)
    }

    /// config from str
    pub fn from_str(s: &str, format: FileFormat) -> Result<Self> {
        let builder = Config::builder();
        let config = builder.add_source(File::from_str(s, format)).build()?;
        let mut setting: Setting = config.try_deserialize()?;
        setting.validate()?;
        Ok(setting)
    }

    fn validate(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use config::FileFormat;
    use std::{fs, thread::sleep, time::Duration};
    use tempfile::Builder;

    #[test]
    fn der() -> Result<()> {
        let json = r#"{
            "lightning": "cln",
            "network": {"port": 1},
            "thread": {"http": 1}
        }"#;

        let mut def = Setting::default();
        def.network.port = 1;
        def.thread.http = 1;
        def.lightning = Lightning::Cln;

        let s2 = serde_json::from_str::<Setting>(json)?;
        let s1: Setting = Setting::from_str(json, FileFormat::Json)?;

        assert_eq!(def, s1);
        assert_eq!(def, s2);

        Ok(())
    }

    #[test]
    fn read() -> Result<()> {
        let setting = Setting::default();
        assert_eq!(setting.network.host, "127.0.0.1");

        let file = Builder::new()
            .prefix("satsbox-config-test-read")
            .suffix(".toml")
            .rand_bytes(0)
            .tempfile()?;

        let setting = Setting::read(&file, None)?;
        assert_eq!(setting.network.host, "127.0.0.1");
        fs::write(
            &file,
            r#"
        [network]
        host = "127.0.0.2"
        "#,
        )?;

        temp_env::with_vars(
            [
                ("ST_network.port", Some("1")),
                ("ST_network__host", Some("127.0.0.3")),
            ],
            || {
                let setting = Setting::read(&file, Some("ST".to_owned())).unwrap();
                assert_eq!(setting.network.host, "127.0.0.3".to_string());
                assert_eq!(setting.network.port, 1);
            },
        );
        Ok(())
    }

    #[test]
    fn watch() -> Result<()> {
        let file = Builder::new()
            .prefix("satsbox-config-test-watch")
            .suffix(".toml")
            .tempfile()?;

        let setting = SettingWrapper::watch(&file, None, |_s| {})?;
        {
            let r = setting.read();
            assert_eq!(r.network.port, 8080);
        }

        fs::write(
            &file,
            r#"[network]
    port = 1
    "#,
        )?;
        sleep(Duration::from_millis(300));
        // println!("read {:?} {:?}", setting.read(), file);
        {
            let r = setting.read();
            assert_eq!(r.network.port, 1);
        }
        Ok(())
    }

    #[test]
    fn fee() -> Result<()> {
        let fee = Fee {
            pay_limit_pct: 0.5,
            small_pay_limit_pct: 1.5,
            internal_pct: 2.5,
            service_pct: 0.3,
        };
        assert_eq!(fee.cal(1000, false), (15, 3));
        assert_eq!(fee.cal(2_000_000, false), (10_000, 6000));
        assert_eq!(fee.cal(1000, true), (25, 3));
        Ok(())
    }
}
