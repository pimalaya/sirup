//! # Parser
//!
//! Top-level clap parser and subcommand dispatcher, resolving the account
//! a command runs against before it opens anything.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use pimalaya_cli::{
    clap::{
        args::{AccountFlag, JsonFlag, LogFlags},
        commands::{
            completion::CompletionCommand, json_schema::JsonSchemaCommand, manual::ManualCommand,
        },
        parsers::path_parser,
    },
    footer, long_version,
    printer::Printer,
    welcome::Welcome,
};
use pimalaya_config::{
    secret::SecretResolver,
    toml::{ConfigMissingError, TomlConfig},
};

use crate::{
    config::{AccountConfig, CONFIG_SAMPLE_URL, Config, ServerConfig},
    json_schema,
    protocol::Protocol,
    repl, session,
};

/// The sample configuration as a raw file, for `curl` to fetch.
const SAMPLE_RAW_URL: &str =
    "https://raw.githubusercontent.com/pimalaya/sirup/master/config.sample.toml";

/// Top-level command-line interface parser.
#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(author, version, about)]
#[command(long_about = concat!(
    "CLI to spawn pre-authenticated IMAP/SMTP/ManageSieve sessions and expose them via Unix sockets.\n\n",
    "First time here? Run `sirup` with no command: it names the commands creating ",
    "the configuration from the documented sample, which you then edit by hand.",
))]
#[command(long_version = long_version!())]
#[command(after_help = footer!())]
#[command(propagate_version = true, infer_subcommands = true)]
pub struct Cli {
    /// The subcommand to run.
    ///
    /// Omitted, a bare `sirup` welcomes a person finding no configuration,
    /// and shows this help otherwise.
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
                let (config, name, account) = resolve_account(config_paths, account_name)?;
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
                    let connection = server.resolve_connection(protocol, &mut secrets)?;

                    upstreams.push(session::open(protocol, sock_path, connection)?);
                }

                drop(secrets);

                session::serve(upstreams)
            }

            Self::Repl(cmd) => {
                let (config, name, account) = resolve_account(config_paths, account_name)?;
                let protocol = select_protocol(&account, &name, cmd.protocol)?;
                let server = account.server(protocol).expect("selected block exists");

                repl::start(protocol, sock_path(&config, &name, protocol, server))
            }

            Self::Manual(cmd) => cmd.execute(printer, Cli::command()),
            Self::Completion(cmd) => cmd.execute(printer, Cli::command()),
            Self::JsonSchema(cmd) => cmd.execute(printer, json_schema::registry()),
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
            "Account `{account_name}` declares no server, add an `imap`, an `smtp` or a `sieve` block: {CONFIG_SAMPLE_URL}"
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

/// Meets a bare `sirup`: a missing configuration fails, so `main` can
/// welcome a person at a terminal, anything else gets the help.
///
/// A broken file counts as a configuration, its error left to the first
/// command reading it.
pub fn meet_bare_invocation(config_paths: &[PathBuf]) -> Result<()> {
    if let Err(err) = Config::load(config_paths)
        && err.is::<ConfigMissingError>()
    {
        return Err(err);
    }

    Cli::command().print_help()?;
    Ok(())
}

/// The welcome a person meets when no configuration exists at `path`:
/// the commands creating it from `sample`, the shipped sample
/// configuration, or downloading it without one, then `start`.
pub fn welcome<'a>(
    path: &'a Path,
    config_paths: &'a [PathBuf],
    sample: Option<&'a Path>,
) -> Welcome<'a> {
    Welcome {
        bin: "sirup",
        intro: "Welcome to Sirup, the CLI to share pre-authenticated IMAP, SMTP and ManageSieve sessions.",
        path,
        config_paths,
        sample,
        sample_url: SAMPLE_RAW_URL,
        next: "Edit it, keep the account and the blocks you want, then open the sessions:",
        command: "start",
    }
}

/// Resolves the account a command runs against, returning the leftover
/// global config, the account name and its config.
///
/// A missing configuration fails with pimalaya-config's
/// `ConfigMissingError`, which `main` meets with the welcome at a
/// terminal. An unknown name lists the accounts the configuration holds,
/// and a missing default names both ways of picking one.
fn resolve_account(
    config_paths: &[PathBuf],
    account_name: Option<&str>,
) -> Result<(Config, String, AccountConfig)> {
    let mut config = Config::load(config_paths)?;
    let (name, account_config) = config.take_account(account_name)?;

    Ok((config, name, account_config))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::cli::welcome;

    #[test]
    #[cfg(not(windows))]
    fn the_welcome_downloads_the_raw_sample_then_starts() {
        let paths = [PathBuf::from("/nonexistent/sirup/config.toml")];
        let welcome = welcome(&paths[0], &paths, None).to_string();

        let curl = "curl -sSL --create-dirs -o '/nonexistent/sirup/config.toml' \
                    https://raw.githubusercontent.com/pimalaya/sirup/master/config.sample.toml";
        assert!(welcome.contains(curl), "{welcome}");
        let start = "\n  sirup -c '/nonexistent/sirup/config.toml' start\n";
        assert!(welcome.contains(start), "{welcome}");
    }

    #[test]
    #[cfg(not(windows))]
    fn the_welcome_copies_the_shipped_sample() {
        let paths = [PathBuf::from("/nonexistent/sirup/config.toml")];
        let sample = Path::new("/usr/share/doc/sirup/config.sample.toml");
        let welcome = welcome(&paths[0], &paths, Some(sample)).to_string();

        let copy = "install -m 644 '/usr/share/doc/sirup/config.sample.toml' \
                    '/nonexistent/sirup/config.toml'";
        assert!(welcome.contains(copy), "{welcome}");
        assert!(!welcome.contains("curl"), "{welcome}");
    }
}
