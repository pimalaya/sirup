//! # Parser
//!
//! Top-level clap parser and subcommand dispatcher, resolving the account
//! a command runs against before it opens anything.

use std::{
    io::{IsTerminal, stdin},
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use pimalaya_cli::{
    clap::{
        args::{AccountFlag, JsonFlag, LogFlags},
        commands::{CompletionCommand, JsonSchemaCommand, ManualCommand},
        parsers::path_parser,
    },
    footer, long_version,
    printer::Printer,
    prompt,
};
use pimalaya_config::toml::TomlConfig;

use crate::{
    config::{AccountConfig, CONFIG_SAMPLE_URL, Config, parse_server},
    json_schema, repl, session,
    wizard::configure::{self, ConfigureCommand},
};

/// Top-level command-line interface parser.
#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(author, version, about)]
#[command(long_about = concat!(
    "CLI to spawn pre-authenticated IMAP/SMTP sessions and expose them via Unix sockets.\n\n",
    "First time here? Run `sirup` with no command: it offers to generate an account ",
    "discovered from your email address, which `sirup configure` does again later. ",
    "Everything discovery does not cover is written by hand.",
))]
#[command(long_version = long_version!())]
#[command(after_help = footer!())]
#[command(propagate_version = true, infer_subcommands = true)]
pub struct Cli {
    /// The subcommand to run.
    ///
    /// Omitted, a bare `sirup` offers to generate a configuration when it
    /// finds none, and shows this help otherwise.
    #[command(subcommand)]
    pub cmd: Option<Command>,
    #[command(flatten)]
    pub config: ConfigPathsArg,
    #[command(flatten)]
    pub account: AccountFlag,
    #[command(flatten)]
    pub json: JsonFlag,
    #[command(flatten)]
    pub log: LogFlags,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start a pre-authenticated IMAP/SMTP session for the given account,
    /// proxied to a Unix socket.
    ///
    /// The protocol is selected from the account's server scheme
    /// (`imap`/`imaps` or `smtp`/`smtps`). This command runs as a blocking
    /// daemon, best placed inside a systemd service or equivalent.
    Start,
    /// Start a basic REPL against the pre-authenticated session for the given
    /// account.
    ///
    /// The REPL connects to the Unix socket spawned by `start` and forwards raw
    /// IMAP or SMTP commands (picked from the account's server scheme). Mostly
    /// intended for testing: it confirms the account is properly configured and
    /// that the socket-backed session is reachable.
    Repl,
    /// Configure an account interactively.
    #[command(visible_alias = "wizard")]
    Configure(ConfigureCommand),
    #[command(alias = "manuals")]
    Manual(ManualCommand),
    #[command(alias = "completions")]
    Completion(CompletionCommand),
    #[command(alias = "json-schemas")]
    JsonSchema(JsonSchemaCommand),
}

/// Path(s) to the TOML configuration file(s).
///
/// Declared here rather than taken from pimalaya-cli, so the environment
/// variable carries this product's name.
#[derive(Debug, Default, Parser)]
pub struct ConfigPathsArg {
    /// Override the default configuration file path.
    ///
    /// Paths are shell-expanded then canonicalized, and several may be given
    /// at once, delimited by `:` like `$PATH`. The first is the base and the
    /// rest are merged on top, which is how a public configuration stays
    /// separate from the private ones.
    #[arg(long = "config", short = 'c', global = true, env = "SIRUP_CONFIG")]
    #[arg(name = "config_paths", value_name = "PATH", value_parser = path_parser, value_delimiter = ':')]
    pub paths: Vec<PathBuf>,
}

impl Command {
    /// Resolves the account the subcommand needs, then runs it.
    pub fn execute(
        self,
        printer: &mut impl Printer,
        config_paths: &[PathBuf],
        account_name: Option<&str>,
    ) -> Result<()> {
        match self {
            Self::Start => {
                let (sock_path, account_config) =
                    resolve_account(printer, config_paths, account_name)?;
                let (server, tls, starttls, sasl) = account_config.resolve_connection()?;
                session::start(sock_path, server, tls, starttls, sasl)
            }

            Self::Repl => {
                let (sock_path, account_config) =
                    resolve_account(printer, config_paths, account_name)?;
                let server = parse_server(&account_config.server)?;
                repl::start(sock_path, server)
            }

            Self::Configure(cmd) => cmd.execute(printer, config_paths),
            Self::Manual(cmd) => cmd.execute(printer, Cli::command()),
            Self::Completion(cmd) => cmd.execute(printer, Cli::command()),
            Self::JsonSchema(cmd) => cmd.execute(printer, json_schema::schemas()),
        }
    }
}

/// Welcomes, then offers to generate a first configuration, returning
/// whether the wizard ran.
///
/// A hook rather than a gate: declining decides nothing, and what happens
/// next is the business of the caller, a bare invocation or a command
/// that needs an account.
pub fn offer_configuration(
    printer: &mut impl Printer,
    config_paths: &[PathBuf],
    path: &Path,
) -> Result<bool> {
    configure::print_welcome(path);

    if !prompt::bool("Create a configuration with a default account?", true)? {
        return Ok(false);
    }

    ConfigureCommand.execute(printer, config_paths)?;

    Ok(true)
}

/// Resolves the account a command runs against, returning the socket it
/// binds or attaches to and the account configuration itself.
///
/// A missing configuration is met with the wizard rather than an error,
/// and the command carries on either way: accepting gives it a chance to
/// work, declining leaves it to fail on the configuration it still has
/// not got.
fn resolve_account(
    printer: &mut impl Printer,
    config_paths: &[PathBuf],
    account_name: Option<&str>,
) -> Result<(PathBuf, AccountConfig)> {
    let mut config = match Config::from_paths_or_default(config_paths)? {
        Some(config) => config,
        None => {
            // NOTE: the target path is where `-c` pointed, so a mistyped
            // path shows up as itself rather than as a generic first run.
            let path = Config::target_path(config_paths)?;

            // NOTE: a script and a JSON consumer cannot answer a prompt,
            // so both skip the offer and fail below.
            if !printer.is_json() && stdin().is_terminal() {
                offer_configuration(printer, config_paths, &path)?;
            }

            // NOTE: the wizard may print the account instead of writing
            // it, so having run it proves nothing and the lookup runs
            // again.
            match Config::from_paths_or_default(config_paths)? {
                Some(config) => config,
                None => bail!(
                    "No configuration found at {}, run `sirup configure` to generate one or write it by hand: {CONFIG_SAMPLE_URL}",
                    path.display(),
                ),
            }
        }
    };

    // NOTE: an empty name and `default` both mean the default account,
    // which is the next block's business.
    let named = account_name.filter(|name| !name.is_empty() && *name != "default");

    if let Some(name) = named.filter(|name| !config.accounts.contains_key(*name)) {
        let mut names: Vec<&str> = config.accounts.keys().map(String::as_str).collect();
        names.sort_unstable();

        bail!(
            "Account `{name}` not found, the configuration holds: {}",
            names.join(", "),
        );
    }

    let Some((name, mut account_config)) = config.take_account(account_name)? else {
        bail!(
            "No default account found, name one with `-a <NAME>` or mark one with `default = true`"
        );
    };

    let sock_path = match account_config.sock_file.take() {
        Some(path) => path,
        None => config.sock_path(&name),
    };

    Ok((sock_path, account_config))
}
