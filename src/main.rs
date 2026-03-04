mod cli;
mod config;
mod repl;
mod session;
mod stream;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use pimalaya_toolbox::{
    config::TomlConfig,
    terminal::{log::Logger, printer::StdoutPrinter},
};

use crate::{
    cli::{SirupCli, SirupCommand},
    config::Config,
};

fn main() -> Result<()> {
    let cli = SirupCli::parse();

    Logger::init(&cli.log);

    match cli.command {
        SirupCommand::Start { account } => {
            let config = Config::from_paths(&cli.config_paths)?;
            let (account_name, mut account_config) = config.get_account(Some(&account.name))?;

            let sock_path = match account_config.sock_file.take() {
                Some(path) => path,
                None => config.sock_path(&account_name),
            };

            session::start(account_config, sock_path)
        }
        SirupCommand::Repl { account } => {
            let config = Config::from_paths(&cli.config_paths)?;
            let (account_name, account_config) = config.get_account(Some(&account.name))?;

            let sock_path = match account_config.sock_file {
                Some(path) => path,
                None => config.sock_path(&account_name),
            };

            let scheme = account_config.url.scheme();

            #[cfg(feature = "imap")]
            if scheme.eq_ignore_ascii_case("imap") || scheme.eq_ignore_ascii_case("imaps") {
                return repl::start_imap(sock_path);
            }

            #[cfg(feature = "smtp")]
            if scheme.eq_ignore_ascii_case("smtp") || scheme.eq_ignore_ascii_case("smtps") {
                return repl::start_smtp(sock_path);
            }

            anyhow::bail!(
                "REPL not available for scheme '{}'. Enable the appropriate feature (imap/smtp).",
                scheme
            )
        }
        SirupCommand::Manuals(cmd) => {
            let mut printer = StdoutPrinter::new(&cli.json);
            cmd.execute(&mut printer, SirupCli::command())
        }
        SirupCommand::Completions(cmd) => {
            let mut printer = StdoutPrinter::new(&cli.json);
            cmd.execute(&mut printer, SirupCli::command())
        }
    }
}
