//! # Configure command
//!
//! The `configure` command: the wizard generates an account, it never
//! edits one.
//!
//! An account is discovered from one prompt, tested, then handed back as
//! a file to create, a block to append or a document on stdout. Whatever
//! discovery does not cover is written by hand against the sample.
//!
//! It runs from `sirup configure`, and from the offer a bare `sirup`
//! raises when it finds no configuration. That offer is the only place
//! the wizard introduces itself, the command asked for by name going
//! straight to the prompts.
//!
//! Appending is a plain text append rather than a re-serialization, so
//! comments, ordering and hand-written formatting survive. Two rules
//! guard it: the account name has to be free, two tables of one name
//! making the whole document fail to parse, and the new account claims
//! the default only when no other one does.

use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{IsTerminal, Write, stdin, stdout},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use pimalaya_cli::{printer::Printer, prompt};
use pimalaya_config::toml::TomlConfig;
use schemars::JsonSchema;
use serde::Serialize;

use crate::config::{AccountConfig, CONFIG_SAMPLE_URL, Config};
#[cfg(discovery)]
use crate::wizard::discover;

/// Configure an account interactively.
///
/// Discovers a provider from an email address, a domain or an
/// `imap[s]://` / `smtp[s]://` URL, tests the connection, then writes the
/// account, appends it to the configuration already there, or prints it
/// to be placed by hand.
///
/// Anything discovery does not cover is written by hand.
#[derive(Debug, Parser)]
pub struct ConfigureCommand;

impl ConfigureCommand {
    /// Runs the wizard, then saves, appends or prints the account.
    ///
    /// No welcome here: whoever typed the command knows what it does, the
    /// banner belonging to the offer a missing configuration raises. The
    /// account name is not asked either, being only the table key.
    ///
    /// A redirected stdout and the JSON output both stay
    /// non-interactive, the document going to stdout and no file being
    /// touched. The prompts render on stderr, out of that document.
    pub fn execute(self, printer: &mut impl Printer, config_paths: &[PathBuf]) -> Result<()> {
        if !stdin().is_terminal() {
            bail!(
                "Configuring needs a terminal to prompt on, \
                 write the configuration by hand instead: {CONFIG_SAMPLE_URL}"
            );
        }

        let path = Config::target_path(config_paths)?;
        let existing = ExistingConfig::read(&path)?;

        let (base_name, mut account) = run_wizard()?;
        let name = account_name(&base_name, existing.as_ref());

        // NOTE: a second `default = true` would make the account every
        // command picks depend on map ordering, so the generated one
        // claims the default only when no other account does.
        let default = !existing.as_ref().is_some_and(|config| config.has_default);
        account.default = default;

        let output = ConfigureOutput {
            document: account.render(&name)?,
            name,
            default,
        };

        if printer.is_json() || !stdout().is_terminal() {
            return printer.out(output);
        }

        match existing {
            Some(_) => append_or_print(printer, &path, output),
            None => save_or_print(printer, &path, output),
        }
    }
}

/// Runs the discovery wizard, returning the account name it suggests and
/// the account it built.
#[cfg(discovery)]
fn run_wizard() -> Result<(String, AccountConfig)> {
    discover::run()
}

#[cfg(not(discovery))]
fn run_wizard() -> Result<(String, AccountConfig)> {
    bail!(
        "The wizard requires the `imap`, `smtp` and a TLS feature \
         (`rustls-ring`, `rustls-aws` or `native-tls`); write the \
         configuration by hand instead: {CONFIG_SAMPLE_URL}"
    )
}

/// What a configuration file already on disk constrains in the generated
/// account: the names it takes, and whether one of its accounts already
/// claims the default.
struct ExistingConfig {
    names: Vec<String>,
    has_default: bool,
}

impl ExistingConfig {
    /// Reads the configuration at the given path, `None` when there is no
    /// file there.
    ///
    /// A file that fails to parse errors rather than read as absent:
    /// appending to a broken document would bury the real problem under a
    /// second one.
    fn read(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }

        let config = Config::from_paths(&[path.to_path_buf()])
            .with_context(|| format!("Read the configuration at {}", path.display()))?;

        Ok(Some(Self {
            names: config.accounts.keys().cloned().collect(),
            has_default: config.accounts.values().any(|account| account.default),
        }))
    }
}

/// The generated account, as the printer takes it.
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigureOutput {
    /// The account name, which is the `[accounts.<name>]` table key.
    name: String,
    /// Whether the account claims the default.
    default: bool,
    /// The rendered TOML document.
    document: String,
}

impl fmt::Display for ConfigureOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // NOTE: the trailing newline terminates the document, and it is
        // also what flushes the line-buffered stdout.
        writeln!(f, "{}", self.document.trim_end())
    }
}

/// Frames Sirup, names the missing configuration file, and points at the
/// sample for what the wizard does not cover.
///
/// It runs before the offer a bare `sirup` raises, where the wizard meets
/// someone who did not ask for it, and `configure` skips it. On stderr,
/// so a redirected stdout holds the document alone.
pub fn print_welcome(path: &Path) {
    eprintln!();
    eprintln!("Welcome to Sirup, the CLI to share pre-authenticated IMAP and SMTP sessions.");
    eprintln!();
    eprintln!("Sirup opens one session against your provider, holds the credentials and the");
    eprintln!("TLS handshake, and exposes the authenticated stream on a Unix socket any local");
    eprintln!("client can speak to. It needs one account to know which session to open, and");
    eprintln!("no configuration file was found at:");
    eprintln!();
    eprintln!("  {}", path.display());
    eprintln!();
    eprintln!("The wizard discovers a provider's settings from your email address, tests the");
    eprintln!("connection and generates a ready-to-use account. Everything discovery does not");
    eprintln!("cover is written by hand, and every field is documented at:");
    eprintln!();
    eprintln!("  {CONFIG_SAMPLE_URL}");
    eprintln!();
    eprintln!("At anytime, you can create a new account with the command:");
    eprintln!();
    eprintln!("  sirup configure");
    eprintln!();
}

/// The name discovery proposes, suffixed until the configuration does not
/// hold it already.
///
/// It is never prompted, being only the table key, but it does have to be
/// free: a second table of one name makes the whole document fail to
/// parse, taking the accounts that used to work down with it.
fn account_name(base: &str, existing: Option<&ExistingConfig>) -> String {
    let taken = existing
        .map(|config| config.names.as_slice())
        .unwrap_or(&[]);

    if !taken.iter().any(|name| name == base) {
        return base.to_string();
    }

    let mut suffix = 2;

    loop {
        let name = format!("{base}-{suffix}");

        if !taken.contains(&name) {
            return name;
        }

        suffix += 1;
    }
}

/// Offers to write the generated account to a configuration file that
/// does not exist yet, printing it instead when the offer is declined.
fn save_or_print(printer: &mut impl Printer, path: &Path, output: ConfigureOutput) -> Result<()> {
    let prompt = format!("Save this account to {}?", path.display());

    if !prompt::bool(prompt, true)? {
        return printer.out(output);
    }

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Create the config directory {}", parent.display()))?;
    }

    fs::write(path, output.to_string())
        .with_context(|| format!("Write the config file {}", path.display()))?;

    print_saved(path, &output);

    Ok(())
}

/// Offers to append the generated account to the configuration file
/// already there, printing it instead when the offer is declined.
fn append_or_print(printer: &mut impl Printer, path: &Path, output: ConfigureOutput) -> Result<()> {
    let prompt = format!("Append account `{}` to {}?", output.name, path.display());

    if !prompt::bool(prompt, true)? {
        return printer.out(output);
    }

    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("Open the config file {}", path.display()))?;

    // NOTE: appending text keeps every comment and hand-written line as
    // they are, which re-serializing the document would not. The leading
    // newline separates the two tables, and terminates the last line of a
    // file that ends without one.
    write!(file, "\n{output}")
        .with_context(|| format!("Append to the config file {}", path.display()))?;

    print_saved(path, &output);

    Ok(())
}

/// Tells where the account landed, under which name, and what to run
/// next.
///
/// The name matters because it was never asked for: an account that did
/// not claim the default is reachable through `-a` alone.
fn print_saved(path: &Path, output: &ConfigureOutput) {
    let name = &output.name;

    eprintln!();
    eprintln!("Account `{name}` saved to {}.", path.display());

    if !output.default {
        eprintln!("Another account holds the default, so name this one with `-a {name}`.");
    }

    eprintln!("Run `sirup start` to open the session and bind its socket.");
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use pimalaya_config::secret::Secret;
    use secrecy::SecretString;

    use super::*;
    use crate::config::{SaslConfig, SaslPlainConfig, TlsConfig};

    static NEXT_CONFIG: AtomicUsize = AtomicUsize::new(0);

    /// A path in the temporary directory no other test writes to.
    fn config_path() -> PathBuf {
        let id = NEXT_CONFIG.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("sirup-configure-{id}.toml"))
    }

    /// The account the wizard builds: one server, one SASL mechanism.
    fn account(default: bool) -> AccountConfig {
        AccountConfig {
            default,
            sock_file: None,
            server: "imaps://mail.example.com:993".into(),
            tls: TlsConfig::default(),
            alpn: None,
            starttls: false,
            sasl: Some(SaslConfig::Plain(SaslPlainConfig {
                authzid: None,
                authcid: "alice@example.com".into(),
                passwd: Secret::Raw(SecretString::from("s3cret")),
            })),
        }
    }

    #[test]
    fn a_generated_account_parses_back() {
        let document = account(true).render("example").expect("render the account");
        let config: Config = toml::from_str(&document).expect("parse the generated config");
        let account = &config.accounts["example"];

        assert_eq!(config.accounts.len(), 1);
        assert!(account.default);
        assert_eq!(account.server, "imaps://mail.example.com:993");

        // NOTE: a generated document holds what was configured, every
        // defaulted field being left out.
        assert!(!document.contains("starttls"));
        assert!(!document.contains("provider"));

        let lines: Vec<&str> = document.lines().collect();
        assert_eq!(lines[0], "[accounts.example]");
        assert_eq!(lines[1], "server = \"imaps://mail.example.com:993\"");
    }

    #[test]
    fn an_appended_account_keeps_the_existing_one() {
        let path = config_path();

        // NOTE: no trailing newline, which is the shape an appended block
        // survives without merging into the last line.
        fs::write(
            &path,
            "# my accounts\n[accounts.work]\ndefault = true\nserver = \"imaps://work.example.com\"",
        )
        .expect("write the existing config");

        let existing = ExistingConfig::read(&path)
            .expect("read the existing config")
            .expect("an existing config");

        assert_eq!(existing.names, ["work"]);
        assert!(existing.has_default);

        let document = account(!existing.has_default)
            .render("example")
            .expect("render the account");
        let mut file = OpenOptions::new().append(true).open(&path).expect("open");
        write!(file, "\n{document}").expect("append the generated account");
        drop(file);

        let content = fs::read_to_string(&path).expect("read back");
        let config: Config = toml::from_str(&content).expect("parse the appended config");

        assert_eq!(config.accounts.len(), 2);

        let defaults = config
            .accounts
            .values()
            .filter(|account| account.default)
            .count();
        assert_eq!(defaults, 1);
        assert!(config.accounts["work"].default);
        assert!(content.starts_with("# my accounts"));

        fs::remove_file(&path).expect("remove the config");
    }

    #[test]
    fn a_taken_name_gets_a_suffix() {
        let existing = ExistingConfig {
            names: vec!["example".to_string(), "example-2".to_string()],
            has_default: true,
        };

        assert_eq!(account_name("example", None), "example");
        assert_eq!(account_name("example", Some(&existing)), "example-3");
        assert_eq!(account_name("work", Some(&existing)), "work");
    }

    #[test]
    fn a_missing_configuration_constrains_nothing() {
        let existing = ExistingConfig::read(&config_path()).expect("read a missing config");

        assert!(existing.is_none());
    }
}
