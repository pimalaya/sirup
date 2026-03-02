mod account;
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
    account::Account,
    cli::{SirupCli, SirupCommand},
    config::Config,
};

fn main() -> Result<()> {
    let cli = SirupCli::parse();

    Logger::init(&cli.log);

    match cli.command {
        SirupCommand::Start { account } => {
            let config = Config::from_paths(&cli.config_paths)?;
            let (_, account_config) = config.get_account(Some(&account.name))?;
            let account = Account::try_from_configs(config, account_config)?;
            session::start(account)
        }
        SirupCommand::Repl { account } => {
            let config = Config::from_paths(&cli.config_paths)?;
            let (_, account_config) = config.get_account(Some(&account.name))?;
            let account = Account::try_from_configs(config, account_config)?;
            repl::start(account)
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
