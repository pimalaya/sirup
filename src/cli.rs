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
use pimalaya_config::{secret::SecretResolver, toml::TomlConfig};

use crate::{
    config::{AccountConfig, CONFIG_SAMPLE_URL, Config, ServerConfig},
    json_schema,
    protocol::Protocol,
    repl, session,
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
    Start(StartCommand),
    Repl(ReplCommand),
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

/// Start a pre-authenticated session per protocol, each proxied to its
/// own Unix socket.
///
/// Every session is opened and authenticated before any socket is bound,
/// so a provider refusing one leaves nothing half-served. This command
/// then runs as a blocking daemon, best placed inside a systemd service
/// or equivalent, and the first session to fail ends the whole run.
#[derive(Debug, Parser)]
pub struct StartCommand {
    /// Protocol(s) to serve, defaulting to every one the account
    /// declares.
    ///
    /// One socket is bound per protocol, so a bare `sirup start` serves
    /// the whole account and `sirup start imap` serves the one block,
    /// which is what a per-protocol service unit wants.
    #[arg(value_name = "PROTOCOL")]
    pub protocols: Vec<Protocol>,
}

/// Start a basic REPL against one of the account's pre-authenticated
/// sessions.
///
/// The REPL connects to the Unix socket `start` bound for that protocol
/// and forwards raw commands. Mostly intended for testing: it confirms
/// the account is properly configured and that the socket-backed session
/// is reachable.
#[derive(Debug, Parser)]
pub struct ReplCommand {
    /// Protocol to attach to, required when the account declares more
    /// than one.
    ///
    /// A single standard input cannot drive two sessions, so this one
    /// takes exactly one protocol where `start` takes a list.
    #[arg(value_name = "PROTOCOL")]
    pub protocol: Option<Protocol>,
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
            Self::Start(cmd) => {
                let (config, name, account) = resolve_account(printer, config_paths, account_name)?;
                let protocols = select_protocols(&account, &name, &cmd.protocols)?;

                // NOTE: one resolver for the whole account, so a
                // credential command both blocks name is spawned once
                // rather than once per block. It holds the plaintext, so
                // it is dropped as soon as every session is open.
                let mut secrets = SecretResolver::new();
                let mut upstreams = Vec::with_capacity(protocols.len());

                for protocol in protocols {
                    let server = account.server(protocol).expect("selected block exists");
                    let sock_path = sock_path(&config, &name, protocol, server);
                    let (url, tls, starttls, sasl) =
                        server.resolve_connection(protocol, &mut secrets)?;

                    upstreams.push(session::open(
                        protocol, sock_path, url, tls, starttls, sasl,
                    )?);
                }

                drop(secrets);

                session::serve(upstreams)
            }

            Self::Repl(cmd) => {
                let (config, name, account) = resolve_account(printer, config_paths, account_name)?;
                let protocol = select_protocol(&account, &name, cmd.protocol)?;
                let server = account.server(protocol).expect("selected block exists");

                repl::start(protocol, sock_path(&config, &name, protocol, server))
            }

            Self::Configure(cmd) => cmd.execute(printer, config_paths),
            Self::Manual(cmd) => cmd.execute(printer, Cli::command()),
            Self::Completion(cmd) => cmd.execute(printer, Cli::command()),
            Self::JsonSchema(cmd) => cmd.execute(printer, json_schema::schemas()),
        }
    }
}

/// The socket one protocol of an account is served on: the block's own
/// `sock-file` when it names one, the derived path otherwise.
fn sock_path(
    config: &Config,
    account_name: &str,
    protocol: Protocol,
    server: &ServerConfig,
) -> PathBuf {
    server
        .sock_file
        .clone()
        .unwrap_or_else(|| config.sock_path(account_name, protocol))
}

/// Resolves the protocols `start` serves: the ones asked for, or every
/// one the account declares.
///
/// An account declaring none has nothing to serve, and a protocol asked
/// for but not declared is a typo worth naming rather than a silently
/// shorter run.
fn select_protocols(
    account: &AccountConfig,
    account_name: &str,
    asked: &[Protocol],
) -> Result<Vec<Protocol>> {
    let declared = declared_protocols(account, account_name)?;

    if asked.is_empty() {
        return Ok(declared);
    }

    let mut selected = Vec::with_capacity(asked.len());

    for protocol in asked {
        if !declared.contains(protocol) {
            bail!(
                "Account `{account_name}` declares no `{protocol}` block, it declares: {}",
                join(&declared),
            );
        }

        if !selected.contains(protocol) {
            selected.push(*protocol);
        }
    }

    Ok(selected)
}

/// Resolves the single protocol `repl` attaches to: the one asked for,
/// or the only one the account declares.
fn select_protocol(
    account: &AccountConfig,
    account_name: &str,
    asked: Option<Protocol>,
) -> Result<Protocol> {
    let declared = declared_protocols(account, account_name)?;

    match asked {
        Some(protocol) if declared.contains(&protocol) => Ok(protocol),
        Some(protocol) => bail!(
            "Account `{account_name}` declares no `{protocol}` block, it declares: {}",
            join(&declared),
        ),
        None if declared.len() == 1 => Ok(declared[0]),
        None => bail!(
            "Account `{account_name}` declares several blocks, name the one to attach to: {}",
            join(&declared),
        ),
    }
}

/// The protocols an account declares a server for, refusing one that
/// declares none.
fn declared_protocols(account: &AccountConfig, account_name: &str) -> Result<Vec<Protocol>> {
    let declared = account.protocols();

    if declared.is_empty() {
        bail!(
            "Account `{account_name}` declares no server, add an `imap` or an `smtp` block: {CONFIG_SAMPLE_URL}"
        );
    }

    Ok(declared)
}

/// Renders a protocol list the way an error message names it.
fn join(protocols: &[Protocol]) -> String {
    protocols
        .iter()
        .map(|protocol| protocol.as_str())
        .collect::<Vec<_>>()
        .join(", ")
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

/// Resolves the account a command runs against, returning the leftover
/// global config, the account name and its config.
///
/// A missing configuration is met with the wizard rather than an error,
/// and the command carries on either way: accepting gives it a chance to
/// work, declining leaves it to fail on the configuration it still has
/// not got.
fn resolve_account(
    printer: &mut impl Printer,
    config_paths: &[PathBuf],
    account_name: Option<&str>,
) -> Result<(Config, String, AccountConfig)> {
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

    let Some((name, account_config)) = config.take_account(account_name)? else {
        bail!(
            "No default account found, name one with `-a <NAME>` or mark one with `default = true`"
        );
    };

    Ok((config, name, account_config))
}
