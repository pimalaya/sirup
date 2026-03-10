// This file is part of Sirup, a CLI to spawn pre-authenticated
// IMAP/SMTP sessions and expose them via Unix sockets.
//
// Copyright (C) 2026 Clément DOUIN <pimalaya.org@posteo.net>
//
// This program is free software: you can redistribute it and/or
// modify it under the terms of the GNU Affero General Public License
// as published by the Free Software Foundation, either version 3 of
// the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see
// <https://www.gnu.org/licenses/>.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use pimalaya_toolbox::{
    config::TomlConfig,
    long_version,
    sasl::{Sasl, SaslAnonymous, SaslLogin, SaslMechanism, SaslPlain},
    stream::{Rustls, RustlsCrypto, Tls, TlsProvider},
    terminal::{
        clap::{
            args::{AccountArg, JsonFlag, LogFlags},
            commands::{CompletionCommand, ManualCommand},
            parsers::path_parser,
        },
        printer::Printer,
    },
};

#[cfg(any(feature = "imap", feature = "smtp"))]
use crate::repl;
use crate::{
    config::{Config, RustlsCryptoConfig, SaslMechanismConfig, TlsProviderConfig},
    session,
};

#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(author, version, about)]
#[command(long_version = long_version!())]
#[command(propagate_version = true, infer_subcommands = true)]
pub struct SirupCli {
    #[command(subcommand)]
    pub command: SirupCommand,
    /// Override the default configuration file path.
    ///
    /// The given paths are shell-expanded then canonicalized (if
    /// applicable). If the first path does not point to a valid file,
    /// the wizard will propose to assist you in the creation of the
    /// configuration file. Other paths are merged with the first one,
    /// which allows you to separate your public config from your
    /// private(s) one(s).
    /// you can also provide multiple paths by delimiting them with a :
    /// like you would when setting $PATH in a posix shell
    #[arg(short, long = "config", global = true, env = "SIRUP_CONFIG")]
    #[arg(value_name = "PATH", value_parser = path_parser, value_delimiter = ':')]
    pub config_paths: Vec<PathBuf>,
    #[command(flatten)]
    pub json: JsonFlag,
    #[command(flatten)]
    pub log: LogFlags,
}

#[derive(Debug, Subcommand)]
pub enum SirupCommand {
    Manuals(ManualCommand),
    Completions(CompletionCommand),

    /// Start a pre-authenticated IMAP session for the given account.
    ///
    /// This command starts a daemon (blocking) for the given account,
    /// best place is inside a systemd service or equivalent.
    Start {
        #[command(flatten)]
        account: AccountArg,
    },
    /// Start a basic REPL on a pre-authenticated IMAP session for the
    /// given account.
    ///
    /// This command mostly stands for testing purpose. It ensures
    /// that the account is properly configured and that it is
    /// possible to connect to the pre-authenticated IMAP session.
    Repl {
        #[command(flatten)]
        account: AccountArg,
    },
}

impl SirupCommand {
    pub fn exec(self, printer: &mut impl Printer, config_paths: &[PathBuf]) -> Result<()> {
        match self {
            SirupCommand::Manuals(cmd) => cmd.execute(printer, SirupCli::command()),
            SirupCommand::Completions(cmd) => cmd.execute(printer, SirupCli::command()),

            SirupCommand::Start { account } => {
                let config = Config::from_paths(config_paths)?;
                let (account_name, mut account_config) = config.get_account(Some(&account.name))?;

                let sock_path = match account_config.sock_file.take() {
                    Some(path) => path,
                    None => config.sock_path(&account_name),
                };

                let url = account_config.url;

                let tls = Tls {
                    provider: match account_config.tls.provider {
                        Some(TlsProviderConfig::Rustls) => Some(TlsProvider::Rustls),
                        Some(TlsProviderConfig::NativeTls) => Some(TlsProvider::NativeTls),
                        None => None,
                    },
                    rustls: Rustls {
                        crypto: match account_config.tls.rustls.crypto {
                            Some(RustlsCryptoConfig::Aws) => Some(RustlsCrypto::Aws),
                            Some(RustlsCryptoConfig::Ring) => Some(RustlsCrypto::Ring),
                            None => None,
                        },
                    },
                    cert: account_config.tls.cert,
                };

                let starttls = account_config.starttls;

                let sasl = Sasl {
                    mechanisms: account_config
                        .sasl
                        .mechanisms
                        .into_iter()
                        .map(|m| match m {
                            SaslMechanismConfig::Login => SaslMechanism::Login,
                            SaslMechanismConfig::Plain => SaslMechanism::Plain,
                            SaslMechanismConfig::Anonymous => SaslMechanism::Anonymous,
                        })
                        .collect(),
                    anonymous: match account_config.sasl.anonymous {
                        Some(auth) => Some(SaslAnonymous {
                            message: auth.message,
                        }),
                        None => None,
                    },
                    login: match account_config.sasl.login {
                        Some(auth) => Some(SaslLogin {
                            username: auth.username,
                            password: auth.password.get()?,
                        }),
                        None => None,
                    },
                    plain: match account_config.sasl.plain {
                        Some(auth) => Some(SaslPlain {
                            authzid: auth.authzid,
                            authcid: auth.authcid,
                            passwd: auth.passwd.get()?,
                        }),
                        None => None,
                    },
                };

                session::start(sock_path, url, tls, starttls, sasl)
            }
            SirupCommand::Repl { account } => {
                let config = Config::from_paths(config_paths)?;
                let (account_name, account_config) = config.get_account(Some(&account.name))?;

                let sock_path = match account_config.sock_file {
                    Some(path) => path,
                    None => config.sock_path(&account_name),
                };

                let scheme = account_config.url.scheme();

                #[cfg(feature = "imap")]
                if scheme.eq_ignore_ascii_case("imap") || scheme.eq_ignore_ascii_case("imaps") {
                    return repl::imap::start(sock_path);
                }

                #[cfg(feature = "smtp")]
                if scheme.eq_ignore_ascii_case("smtp") || scheme.eq_ignore_ascii_case("smtps") {
                    return repl::smtp::start(sock_path);
                }

                bail!(
                    "REPL not available for scheme '{}'. Enable the appropriate feature (imap/smtp).",
                    scheme
		)
            }
        }
    }
}
