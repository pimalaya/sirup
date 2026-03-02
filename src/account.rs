use std::path::PathBuf;

use anyhow::Result;
use log::warn;
#[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
use rustls::crypto::CryptoProvider;
use secrecy::SecretString;

use crate::config::{
    AccountConfig, Config, RustlsCryptoConfig, SaslMechanismConfig, TlsProviderConfig,
};

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
            let provider = account_config.tls.provider.unwrap_or({
                #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
                { TlsProviderConfig::Rustls }
                #[cfg(all(feature = "native-tls", not(feature = "rustls-aws"), not(feature = "rustls-ring")))]
                { TlsProviderConfig::NativeTls }
                #[cfg(not(any(feature = "native-tls", feature = "rustls-aws", feature = "rustls-ring")))]
                anyhow::bail!("no TLS provider available: enable the `native-tls`, `rustls-ring`, or `rustls-aws` feature")
            });

            match provider {
                TlsProviderConfig::Rustls => {
                    #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
                    {
                        let crypto = account_config.tls.rustls.crypto.unwrap_or({
                            #[cfg(feature = "rustls-ring")]
                            {
                                RustlsCryptoConfig::Ring
                            }
                            #[cfg(all(feature = "rustls-aws", not(feature = "rustls-ring")))]
                            {
                                RustlsCryptoConfig::Aws
                            }
                        });
                        let provider = match crypto {
                            RustlsCryptoConfig::Ring => {
                                #[cfg(feature = "rustls-ring")]
                                {
                                    rustls::crypto::ring::default_provider()
                                }
                                #[cfg(not(feature = "rustls-ring"))]
                                anyhow::bail!("rustls crypto provider `ring` requires the `rustls-ring` feature")
                            }
                            RustlsCryptoConfig::Aws => {
                                #[cfg(feature = "rustls-aws")]
                                {
                                    rustls::crypto::aws_lc_rs::default_provider()
                                }
                                #[cfg(not(feature = "rustls-aws"))]
                                anyhow::bail!("rustls crypto provider `aws` requires the `rustls-aws` feature")
                            }
                        };
                        Tls::Rustls {
                            starttls: account_config.starttls,
                            cert: account_config.tls.cert,
                            provider,
                        }
                    }
                    #[cfg(not(any(feature = "rustls-aws", feature = "rustls-ring")))]
                    anyhow::bail!(
                        "TLS provider `rustls` requires the `rustls-ring` or `rustls-aws` feature"
                    )
                }
                TlsProviderConfig::NativeTls => {
                    #[cfg(feature = "native-tls")]
                    {
                        Tls::NativeTls {
                            starttls: account_config.starttls,
                            cert: account_config.tls.cert,
                        }
                    }
                    #[cfg(not(feature = "native-tls"))]
                    anyhow::bail!("TLS provider `native-tls` requires the `native-tls` feature")
                }
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
    #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
    Rustls {
        starttls: bool,
        cert: Option<PathBuf>,
        provider: CryptoProvider,
    },
    #[cfg(feature = "native-tls")]
    NativeTls {
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
