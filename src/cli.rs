// This file is part of Sirup, a CLI to spawn pre-authenticated IMAP/SMTP
// sessions and expose them via Unix sockets.
//
// Copyright (C) 2026 Clément DOUIN <pimalaya.org@posteo.net>
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option) any
// later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use pimalaya_cli::{
    clap::{
        args::{AccountArg, JsonFlag, LogFlags},
        commands::{CompletionCommand, ManualCommand},
        parsers::path_parser,
    },
    long_version,
    printer::Printer,
};

use pimalaya_config::toml::TomlConfig;

#[cfg(any(feature = "imap", feature = "smtp"))]
use crate::repl;
use crate::{config::Config, session};

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
    /// Start a pre-authenticated IMAP/SMTP session for the given
    /// account, proxied to a Unix socket.
    ///
    /// The protocol is selected from the account's URL scheme
    /// (`imap`/`imaps` or `smtp`/`smtps`). This command runs as a
    /// blocking daemon; best place is inside a systemd service or
    /// equivalent.
    Start {
        #[command(flatten)]
        account: AccountArg,
    },
    /// Start a basic REPL against the pre-authenticated session for
    /// the given account.
    ///
    /// The REPL connects to the Unix socket spawned by `start` and
    /// forwards raw IMAP or SMTP commands (picked from the account's
    /// URL scheme). Mostly intended for testing: it confirms the
    /// account is properly configured and that the socket-backed
    /// session is reachable.
    Repl {
        #[command(flatten)]
        account: AccountArg,
    },

    Manuals(ManualCommand),
    Completions(CompletionCommand),
}

impl SirupCommand {
    pub fn execute(self, printer: &mut impl Printer, config_paths: &[PathBuf]) -> Result<()> {
        match self {
            SirupCommand::Start { account } => {
                let mut config = Config::from_paths(config_paths)?;
                let accounts = config.take_account(Some(&account.name))?;

                let Some((account_name, mut account_config)) = accounts else {
                    bail!("Cannot find account `{}`", account.name)
                };

                let sock_path = match account_config.sock_file.take() {
                    Some(path) => path,
                    None => config.sock_path(&account_name),
                };

                let url = account_config.url;
                let tls = account_config.tls.into();
                let starttls = account_config.starttls;
                let sasl = account_config.sasl.map(TryInto::try_into).transpose()?;

                session::start(sock_path, url, tls, starttls, sasl)
            }
            SirupCommand::Repl { account } => {
                let mut config = Config::from_paths(config_paths)?;
                let accounts = config.take_account(Some(&account.name))?;

                let Some((account_name, account_config)) = accounts else {
                    bail!("Cannot find account `{}`", account.name)
                };

                let sock_path = match account_config.sock_file {
                    Some(path) => path,
                    None => config.sock_path(&account_name),
                };

                match account_config.url.scheme() {
                    #[cfg(feature = "imap")]
                    "imap" | "imaps" => repl::imap::start(sock_path),
                    #[cfg(not(feature = "imap"))]
                    "imap" | "imaps" => bail!("missing cargo feature: `imap`"),
                    #[cfg(feature = "smtp")]
                    "smtp" | "smtps" => repl::smtp::start(sock_path),
                    #[cfg(not(feature = "smtp"))]
                    "smtp" | "smtps" => bail!("missing cargo feature: `smtp`"),
                    s => bail!("unknown scheme `{s}`, expects `imap(s)` or `smtp(s)`"),
                }
            }

            SirupCommand::Manuals(cmd) => cmd.execute(printer, SirupCli::command()),
            SirupCommand::Completions(cmd) => cmd.execute(printer, SirupCli::command()),
        }
    }
}
