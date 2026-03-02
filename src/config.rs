use std::{collections::HashMap, env::temp_dir, fs, path::PathBuf};

use log::{error, warn};
use pimalaya_toolbox::{config::TomlConfig, secret::Secret};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_socks_dir")]
    pub socks_dir: PathBuf,
    pub accounts: HashMap<String, AccountConfig>,
}

fn default_socks_dir() -> PathBuf {
    if let Some(path) = dirs::runtime_dir() {
        return path;
    }

    let path = temp_dir().join(format!("service-{}", env!("CARGO_PKG_NAME")));
    let p = path.display();

    warn!("runtime dir not found, falling back to {p}");

    if let Err(err) = fs::create_dir_all(&path) {
        error!("cannot create dir {p} ({err}), assuming it already exists");
    }

    path
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AccountConfig {
    pub sock_file: Option<PathBuf>,
    pub host: String,
    pub port: Option<u16>,
    #[serde(default)]
    pub tls: TlsConfig,
    #[serde(default)]
    pub starttls: bool,
    #[serde(default)]
    pub sasl: SaslConfig,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TlsConfig {
    #[serde(default)]
    pub disable: bool,
    pub provider: Option<TlsProviderConfig>,
    #[serde(default)]
    pub rustls: RustlsConfig,
    pub cert: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum TlsProviderConfig {
    Rustls,
    NativeTls,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RustlsConfig {
    pub crypto: Option<RustlsCryptoConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum RustlsCryptoConfig {
    Aws,
    Ring,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslConfig {
    #[serde(default = "SaslConfig::default_mechanisms")]
    pub mechanisms: Vec<SaslMechanismConfig>,
    pub login: Option<SaslLoginConfig>,
    pub plain: Option<SaslPlainConfig>,
    pub anonymous: Option<SaslAnonymousConfig>,
}

impl SaslConfig {
    fn default_mechanisms() -> Vec<SaslMechanismConfig> {
        vec![SaslMechanismConfig::Plain, SaslMechanismConfig::Login]
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SaslMechanismConfig {
    Login,
    Plain,
    Anonymous,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslLoginConfig {
    pub username: String,
    pub password: Secret,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslPlainConfig {
    pub authzid: Option<String>,
    pub authcid: String,
    pub passwd: Secret,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslAnonymousConfig {
    pub message: Option<String>,
}

impl TomlConfig for Config {
    type Account = AccountConfig;

    fn project_name() -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    fn find_default_account(&self) -> Option<(String, Self::Account)> {
        None
    }

    fn find_account(&self, name: &str) -> Option<(String, Self::Account)> {
        self.accounts
            .get(name)
            .map(|account| (name.to_owned(), account.clone()))
    }
}
