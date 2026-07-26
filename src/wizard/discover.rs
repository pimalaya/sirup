//! In-memory wizard flow.
//!
//! 1. Ask once for an email address, an `imap[s]://` / `smtp[s]://`
//!    URL, or a bare domain.
//! 2. URL input: scheme picks the protocol; host/port/TLS come straight
//!    from the URL, no extra prompt.
//! 3. Email / domain input: probe PACC → Autoconfig ISP (when an email
//!    was given) → Autoconfig ISP-fallback → Autoconfig ISPDB → RFC
//!    6186 SRV; first non-empty wins. If both IMAP and SMTP come back,
//!    ask which one to start.
//! 4. Prompt the SASL mechanism plus only the fields it needs; secrets
//!    go through the shared keyring/command/raw picker.
//! 5. Test the account by connecting once, then print it as a
//!    ready-to-save config fragment on stdout; nothing is persisted.

use std::{collections::BTreeMap, env, fmt};

use anyhow::{Result, bail};
use io_pim_discovery::shared::dns::system_resolver;
use pimalaya_cli::{
    printer::Printer,
    prompt,
    spinner::Spinner,
    wizard::{
        imap::{Encryption as ImapEncryption, WizardImapConfig},
        smtp::{Encryption as SmtpEncryption, WizardSmtpConfig},
    },
};
use pimalaya_config::toml as config_toml;
use pimalaya_stream::tls::Tls;
use serde::Serialize;
use url::Url;

use crate::{
    config::{
        AccountConfig, SaslAnonymousConfig, SaslConfig, SaslLoginConfig, SaslOauthbearerConfig,
        SaslPlainConfig, SaslScramSha256Config, SaslXoauth2Config, TlsConfig,
    },
    wizard::{autoconfig, pacc, secret, srv},
};

/// DNS-over-TCP resolver backing discovery when `SIRUP_DNS_RESOLVER`
/// is unset and no system resolver is found: Cloudflare's `1.1.1.1`.
const DEFAULT_RESOLVER: &str = "tcp://1.1.1.1:53";

/// Resolver used by discovery: the `SIRUP_DNS_RESOLVER` override
/// first, then the system resolver (`/etc/resolv.conf` on unix, the
/// network adapters on windows), then the Cloudflare default. This
/// avoids leaking the queried domain to a third-party resolver and
/// works around networks that block the default.
pub fn discovery_resolver() -> Url {
    if let Ok(resolver) = env::var("SIRUP_DNS_RESOLVER") {
        if let Ok(url) = resolver.parse() {
            return url;
        }
    }

    if let Some(url) = system_resolver() {
        return url;
    }

    DEFAULT_RESOLVER
        .parse()
        .expect("DEFAULT_RESOLVER must be a valid URL")
}

/// TLS profile for the HTTPS-bound discovery probes; they only speak
/// HTTP/1.1 to the `.well-known` endpoints.
pub fn discovery_tls() -> Tls {
    let mut tls = Tls::default();
    tls.rustls.alpn = vec!["http/1.1".into()];
    tls
}

/// Per-source discovery payload. Sirup only routes IMAP/SMTP, so any
/// JMAP endpoint a probe might surface is dropped at the source.
#[derive(Default)]
pub struct DiscoveryResult {
    pub imap: Option<WizardImapConfig>,
    pub smtp: Option<WizardSmtpConfig>,
}

impl DiscoveryResult {
    /// Whether neither an IMAP nor an SMTP endpoint was found, marking
    /// the source as a miss so the discovery chain moves on.
    pub fn is_empty(&self) -> bool {
        self.imap.is_none() && self.smtp.is_none()
    }
}

/// Prompts once for an email address, a server URL or a bare domain,
/// builds the account from it, then prints it as a ready-to-save config
/// fragment on stdout. Run on bare `sirup`. Writes nothing to disk: the
/// user redirects the output into their config (e.g.
/// `sirup >> <config>`), so prompts render on stderr and only the
/// fragment lands on stdout.
pub fn run(printer: &mut impl Printer) -> Result<()> {
    let input = prompt::text::<&str>("Email, server or URL:", None)?;
    let input = input.trim();
    if input.is_empty() {
        bail!("Empty input: enter an email address, a server URL or a domain");
    }

    // NOTE: the account name is only the TOML table key, so it is
    // derived from the input rather than prompted; the user renames it
    // by hand. It also seeds the keyring entry for stored secrets.
    let name = default_account_name(input);
    let account = build_account(input, &name)?;

    // Test the account before printing it, exactly like himalaya: a bad
    // credential or endpoint fails here and stops the process, rather
    // than emitting a config that cannot connect.
    let spinner = Spinner::start("Testing account configuration");
    if let Err(err) = crate::test_account(&account) {
        spinner.failure("Account configuration test failed");
        return Err(err);
    }
    spinner.success("Account configuration is valid");

    printer.out(GeneratedConfig::new(name, account))
}

/// Derives the `[accounts.<name>]` table key suggested for `input`: the
/// first label of the email domain, of the URL host, or of a bare
/// domain. Only a suggestion, the user renames it in the printed
/// fragment.
fn default_account_name(input: &str) -> String {
    if let Some((_, domain)) = input.rsplit_once('@') {
        if !input.contains("://") {
            return first_label(domain);
        }
    }

    if let Ok(url) = Url::parse(input) {
        if let Some(host) = url.host_str() {
            return first_label(host);
        }
    }

    first_label(input)
}

/// First dot-separated label of `host`, falling back to `account` when
/// it is empty.
fn first_label(host: &str) -> String {
    host.split('.')
        .next()
        .filter(|label| !label.is_empty())
        .unwrap_or("account")
        .to_string()
}

/// Builds an account from an already-collected input, routing by its
/// shape: a URL is used as-is, an email or bare domain runs discovery.
fn build_account(input: &str, account_name: &str) -> Result<AccountConfig> {
    match classify(input)? {
        Input::Url(url) => build_url_account(url, account_name),
        Input::Domain(domain) => build_discovery_account(None, &domain, account_name),
        Input::Email { local, domain } => {
            build_discovery_account(Some(&local), &domain, account_name)
        }
    }
}

enum Input {
    Email { local: String, domain: String },
    Url(Url),
    Domain(String),
}

fn classify(input: &str) -> Result<Input> {
    if input.is_empty() {
        bail!("Empty input");
    }

    if input.contains('@') && !input.contains("://") {
        let Some((local, domain)) = input.rsplit_once('@') else {
            bail!("Invalid email address `{input}`")
        };
        return Ok(Input::Email {
            local: local.to_owned(),
            domain: domain.to_owned(),
        });
    }

    match Url::parse(input) {
        Ok(url) => Ok(Input::Url(url)),
        Err(url::ParseError::RelativeUrlWithoutBase) => Ok(Input::Domain(input.to_owned())),
        Err(err) => Err(err.into()),
    }
}

/// Builds an account straight from an `imap[s]://` / `smtp[s]://` URL:
/// validates the scheme and host, derives STARTTLS from the plain
/// scheme, then prompts only for the SASL credentials.
fn build_url_account(url: Url, account_name: &str) -> Result<AccountConfig> {
    let scheme = url.scheme().to_ascii_lowercase();
    if url.host_str().is_none() {
        bail!("URL `{url}` is missing a host")
    }

    match scheme.as_str() {
        // Direct IMAP/SMTP URL: use as-is, just prompt for SASL.
        "imap" | "imaps" | "smtp" | "smtps" => {
            let starttls = matches!(scheme.as_str(), "imap" | "smtp");
            let sasl = prompt_sasl(account_name, None)?;
            Ok(AccountConfig {
                // NOTE: left non-default like himalaya, so it does not
                // hijack the default when merged into a config that
                // already has one. Being false, `default` is omitted from
                // the printed fragment; the user marks their choice with
                // `default = true`.
                default: false,
                sock_file: None,
                server: url.to_string(),
                tls: TlsConfig::default(),
                alpn: None,
                starttls,
                sasl: Some(sasl),
            })
        }
        other => bail!("Unsupported URL scheme `{other}`, expects `imap(s)` or `smtp(s)`"),
    }
}

/// Builds an account from a discovered endpoint: runs the first-hit
/// discovery chain over the domain, picks the protocol (prompting only
/// when both IMAP and SMTP come back), then prompts for the SASL
/// credentials. Bails when nothing is discovered.
fn build_discovery_account(
    local_part: Option<&str>,
    domain: &str,
    account_name: &str,
) -> Result<AccountConfig> {
    let result = discover(local_part, domain);
    if result.is_empty() {
        bail!(
            "No configuration could be discovered for `{domain}`. \
             Try giving an `imap[s]://` or `smtp[s]://` URL instead."
        );
    }

    let DiscoveryResult { imap, smtp } = result;
    let login_default = local_part.map(|l| format!("{l}@{domain}"));

    let protocol = choose_protocol(imap.is_some(), smtp.is_some())?;
    match protocol {
        Protocol::Imap => {
            let endpoint = imap.expect("imap endpoint must be present when chosen");
            let sasl = prompt_sasl(account_name, login_default.as_deref())?;
            Ok(build_imap_account(endpoint, sasl))
        }
        Protocol::Smtp => {
            let endpoint = smtp.expect("smtp endpoint must be present when chosen");
            let sasl = prompt_sasl(account_name, login_default.as_deref())?;
            Ok(build_smtp_account(endpoint, sasl))
        }
    }
}

#[derive(Clone, Copy)]
enum Protocol {
    Imap,
    Smtp,
}

fn choose_protocol(has_imap: bool, has_smtp: bool) -> Result<Protocol> {
    match (has_imap, has_smtp) {
        (true, false) => Ok(Protocol::Imap),
        (false, true) => Ok(Protocol::Smtp),
        (true, true) => {
            let pick = prompt::item("Protocol to start:", ["IMAP", "SMTP"], Some("IMAP"))?;
            Ok(if pick == "IMAP" {
                Protocol::Imap
            } else {
                Protocol::Smtp
            })
        }
        (false, false) => bail!("Discovery returned no IMAP nor SMTP endpoint"),
    }
}

/// Probes PACC → Autoconfig ISP (when `local_part` is `Some`) →
/// Autoconfig ISP-fallback → Thunderbird ISPDB → RFC 6186 SRV in that
/// order, returning the first non-empty result.
fn discover(local_part: Option<&str>, domain: &str) -> DiscoveryResult {
    if let Some(result) = pacc::run(domain)
        .map(|c| pacc::defaults(&c))
        .filter(|r| !r.is_empty())
    {
        return result;
    }

    if let Some(local) = local_part {
        if let Some(result) = autoconfig::run_isp(local, domain)
            .map(|c| autoconfig::defaults(&c))
            .filter(|r| !r.is_empty())
        {
            return result;
        }
    }

    if let Some(result) = autoconfig::run_isp_fallback(domain)
        .map(|c| autoconfig::defaults(&c))
        .filter(|r| !r.is_empty())
    {
        return result;
    }

    if let Some(result) = autoconfig::run_ispdb(domain)
        .map(|c| autoconfig::defaults(&c))
        .filter(|r| !r.is_empty())
    {
        return result;
    }

    if let Some(result) = srv::run(domain)
        .map(|r| srv::defaults(&r))
        .filter(|r| !r.is_empty())
    {
        return result;
    }

    DiscoveryResult::default()
}

/// SASL mechanisms offered by the credential prompt, in menu order.
const SASL_MECHANISMS: [&str; 6] = [
    "PLAIN",
    "LOGIN",
    "XOAUTH2",
    "OAUTHBEARER",
    "SCRAM-SHA-256",
    "ANONYMOUS",
];

/// Prompts for the SASL mechanism and the fields it needs. Passwords and
/// tokens go through the shared OS-aware pickers (keyring, custom command
/// or raw), never a bare raw prompt; `account_name` seeds the keyring
/// entry.
fn prompt_sasl(account_name: &str, login_default: Option<&str>) -> Result<SaslConfig> {
    let mechanism = prompt::item("SASL mechanism:", SASL_MECHANISMS, Some("PLAIN"))?;

    Ok(match mechanism {
        "PLAIN" => SaslConfig::Plain(SaslPlainConfig {
            authzid: None,
            authcid: prompt::text("Login:", login_default)?,
            passwd: secret::configure_password("Password", account_name)?,
        }),
        "LOGIN" => SaslConfig::Login(SaslLoginConfig {
            username: prompt::text("Username:", login_default)?,
            password: secret::configure_password("Password", account_name)?,
        }),
        "XOAUTH2" => SaslConfig::Xoauth2(SaslXoauth2Config {
            username: prompt::text("Username:", login_default)?,
            token: secret::configure_token("Access token", account_name, true)?,
        }),
        "OAUTHBEARER" => SaslConfig::Oauthbearer(SaslOauthbearerConfig {
            username: prompt::text("Username:", login_default)?,
            token: secret::configure_token("Access token", account_name, true)?,
        }),
        "SCRAM-SHA-256" => SaslConfig::ScramSha256(SaslScramSha256Config {
            username: prompt::text("Username:", login_default)?,
            password: secret::configure_password("Password", account_name)?,
        }),
        "ANONYMOUS" => SaslConfig::Anonymous(SaslAnonymousConfig {
            message: prompt::some_text::<&str>("Anonymous message (optional):", None)?,
        }),
        _ => unreachable!(),
    })
}

/// Assembles an IMAP account from a discovered endpoint, deriving the
/// scheme and STARTTLS switch from its encryption.
fn build_imap_account(endpoint: WizardImapConfig, sasl: SaslConfig) -> AccountConfig {
    let starttls = matches!(endpoint.encryption, ImapEncryption::StartTls);
    let scheme = if starttls { "imap" } else { "imaps" };
    let server = format!("{scheme}://{}:{}", endpoint.host, endpoint.port);

    AccountConfig {
        // Left non-default like himalaya (see build_url_account); omitted
        // from the fragment while false.
        default: false,
        sock_file: None,
        server,
        tls: TlsConfig::default(),
        alpn: None,
        starttls,
        sasl: Some(sasl),
    }
}

/// Assembles an SMTP account from a discovered endpoint, deriving the
/// scheme and STARTTLS switch from its encryption.
fn build_smtp_account(endpoint: WizardSmtpConfig, sasl: SaslConfig) -> AccountConfig {
    let starttls = matches!(endpoint.encryption, SmtpEncryption::StartTls);
    let scheme = if starttls { "smtp" } else { "smtps" };
    let server = format!("{scheme}://{}:{}", endpoint.host, endpoint.port);

    AccountConfig {
        // Left non-default like himalaya (see build_url_account); omitted
        // from the fragment while false.
        default: false,
        sock_file: None,
        server,
        tls: TlsConfig::default(),
        alpn: None,
        starttls,
        sasl: Some(sasl),
    }
}

/// The account produced by the wizard, printed as a ready-to-save
/// `[accounts.<name>]` fragment on stdout with its guidance embedded as
/// comments, or the same data serialized as an object in JSON mode. The
/// wizard writes nothing itself: the user redirects the output into
/// their config file (e.g. `sirup >> <config>`), so prompts go to
/// stderr and only this lands on stdout.
#[derive(Serialize)]
struct GeneratedConfig {
    accounts: BTreeMap<String, AccountConfig>,
}

impl GeneratedConfig {
    /// Wraps a single wizard-built account under its table key.
    fn new(name: String, account: AccountConfig) -> Self {
        Self {
            accounts: BTreeMap::from([(name, account)]),
        }
    }
}

impl fmt::Display for GeneratedConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let toml = config_toml::to_string(self).map_err(|_| fmt::Error)?;

        writeln!(f, "# Account generated by the sirup wizard.")?;
        writeln!(f, "#")?;
        writeln!(f, "# Nothing was written to disk: save this into your")?;
        writeln!(f, "# config file, one of:")?;
        writeln!(f, "#   $XDG_CONFIG_HOME/sirup/config.toml")?;
        writeln!(f, "#   $HOME/.config/sirup/config.toml")?;
        writeln!(f, "#   $HOME/.siruprc")?;
        writeln!(f, "#")?;
        writeln!(
            f,
            "# Prompts render on stderr, so redirecting works directly:"
        )?;
        writeln!(f, "#   sirup >> ~/.config/sirup/config.toml")?;
        writeln!(f, "#")?;
        writeln!(
            f,
            "# The account name (the [accounts.*] table key) is derived"
        )?;
        writeln!(f, "# from your input; rename it to anything you like.")?;
        writeln!(f, "#")?;
        writeln!(f, "# Every field is documented in the sample config:")?;
        writeln!(
            f,
            "# https://github.com/pimalaya/sirup/blob/master/config.sample.toml"
        )?;
        writeln!(f)?;
        write!(f, "{toml}")
    }
}

#[cfg(test)]
mod tests {
    use pimalaya_config::secret::Secret;
    use secrecy::SecretString;

    use super::*;
    use crate::config::{SaslConfig, SaslPlainConfig, TlsConfig};

    #[test]
    fn generated_config_renders_account_fragment() {
        let account = AccountConfig {
            default: false,
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
        };

        let rendered = GeneratedConfig::new("example".into(), account).to_string();

        assert!(rendered.contains("[accounts.example]"));
        assert!(rendered.contains("server = \"imaps://mail.example.com:993\""));
        assert!(rendered.contains("sasl.plain.authcid = \"alice@example.com\""));
        assert!(rendered.contains("sasl.plain.passwd.raw = \"s3cret\""));
        // A non-default account, a false switch and a default TLS block are
        // all omitted from the fragment (himalaya-style).
        assert!(!rendered.contains("default"));
        assert!(!rendered.contains("starttls"));
        assert!(!rendered.contains("provider"));
    }
}
