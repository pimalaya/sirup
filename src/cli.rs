use std::path::PathBuf;

use clap::{Parser, Subcommand};
use pimalaya_toolbox::{
    long_version,
    terminal::clap::{
        args::{AccountArg, JsonFlag, LogFlags},
        commands::{CompletionCommand, ManualCommand},
        parsers::path_parser,
    },
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
    Manuals(ManualCommand),
    Completions(CompletionCommand),
}
