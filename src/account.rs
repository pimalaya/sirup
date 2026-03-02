use std::path::PathBuf;

use anyhow::Result;
use log::warn;
use secrecy::SecretString;

use crate::config::{AccountConfig, Config, SaslMechanismConfig};

#[derive(Clone, Debug)]
pub struct Account {
    pub sock_path: PathBuf,
    pub host: String,
    pub port: u16,
    pub tls: Tls,
    pub sasl: Vec<SaslCandidate>,
}

impl Account {
    pub fn try_from_configs(config: Config, mut account_config: AccountConfig) -> Result<Self> {
        let sock_path = match account_config.sock_file {
            Some(path) => path,
            None => config
                .socks_dir
                .join(format!("{}.sock", env!("CARGO_PKG_NAME"))),
        };

        let port = match account_config.port {
            Some(port) => port,
            None => {
                if account_config.tls.disable {
                    143
                } else {
                    993
                }
            }
        };

        let tls = if account_config.tls.disable {
            Tls::None
        } else {
            Tls::Rustls {
                starttls: account_config.starttls,
                cert: account_config.tls.cert,
            }
        };

        let mut sasl = vec![];

        for mechanism in account_config.sasl.mechanisms {
            match mechanism {
                SaslMechanismConfig::Login => {
                    let Some(auth) = account_config.sasl.login.take() else {
                        warn!("missing SASL LOGIN configuration, skipping it");
                        continue;
                    };

                    sasl.push(SaslCandidate::Login {
                        username: auth.username,
                        password: auth.password.get()?,
                    });
                }
                SaslMechanismConfig::Plain => {
                    let Some(auth) = account_config.sasl.plain.take() else {
                        warn!("missing SASL PLAIN configuration, skipping it");
                        continue;
                    };

                    sasl.push(SaslCandidate::Plain {
                        authzid: auth.authzid,
                        authcid: auth.authcid,
                        passwd: auth.passwd.get()?,
                    });
                }
                SaslMechanismConfig::Anonymous => {
                    let message = account_config
                        .sasl
                        .anonymous
                        .take()
                        .and_then(|auth| auth.message)
                        .unwrap_or_default();

                    sasl.push(SaslCandidate::Anonymous { message });
                }
            };
        }

        Ok(Self {
            sock_path,
            host: account_config.host,
            port,
            tls,
            sasl,
        })
    }
}

#[derive(Clone, Debug)]
pub enum Tls {
    None,
    Rustls {
        starttls: bool,
        cert: Option<PathBuf>,
    },
}

#[derive(Clone, Debug)]
pub enum SaslCandidate {
    Anonymous {
        message: String,
    },
    Login {
        username: String,
        password: SecretString,
    },
    Plain {
        authzid: Option<String>,
        authcid: String,
        passwd: SecretString,
    },
}
