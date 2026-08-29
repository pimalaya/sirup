//! In-memory wizard flow.
//!
//! 1. Ask once for an email address, an `imap[s]://`, `smtp[s]://` or
//!    `sieve[s]://` URL, or a bare domain.
//! 2. URL input: scheme picks the protocol; host/port/TLS come straight
//!    from the URL, no extra prompt.
//! 3. Email / domain input: probe PACC → Autoconfig ISP (when an email
//!    was given) → Autoconfig ISP-fallback → Autoconfig ISPDB → RFC
//!    6186 SRV; first non-empty wins. Whatever comes back is kept, an
//!    account declaring as many blocks as it speaks protocols.
//! 4. Prompt the SASL mechanism plus only the fields it needs, once for
//!    the whole account; secrets go through the shared
//!    keyring/command/raw picker.
//! 5. Test every block by connecting once, then hand the account back
//!    with the name it suggests. What becomes of it belongs to
//!    [`crate::wizard::configure`].

use std::env;

use anyhow::{Result, bail};
use io_pim_discovery::shared::dns::system_resolver;
use pimalaya_cli::{
    prompt,
    spinner::Spinner,
    wizard::{
        imap::{Encryption as ImapEncryption, WizardImapConfig},
        smtp::{Encryption as SmtpEncryption, WizardSmtpConfig},
    },
};
use pimalaya_config::secret::SecretResolver;
use pimalaya_stream::tls::Tls;
use url::Url;

use crate::{
    config::{
        AccountConfig, SaslAnonymousConfig, SaslConfig, SaslLoginConfig, SaslOauthbearerConfig,
        SaslPlainConfig, SaslScramSha256Config, SaslXoauth2Config, ServerConfig, TlsConfig,
    },
    protocol::Protocol,
    session,
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
    if let Ok(resolver) = env::var("SIRUP_DNS_RESOLVER")
        && let Ok(url) = resolver.parse()
    {
        return url;
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

/// Per-source discovery payload. Sirup only routes the SASL-mediated
/// mail protocols, so any JMAP endpoint a probe might surface is
/// dropped at the source.
#[derive(Default)]
pub struct DiscoveryResult {
    pub imap: Option<WizardImapConfig>,
    pub smtp: Option<WizardSmtpConfig>,
    pub sieve: Option<SieveEndpoint>,
}

impl DiscoveryResult {
    /// Whether no endpoint at all was found, marking the source as a
    /// miss so the discovery chain moves on.
    pub fn is_empty(&self) -> bool {
        self.imap.is_none() && self.smtp.is_none() && self.sieve.is_none()
    }
}

/// A discovered ManageSieve endpoint.
///
/// pimalaya-cli's wizard carries an IMAP and an SMTP shape but no
/// ManageSieve one, ManageSieve being the protocol none of its prompts
/// covers, so the three fields sirup needs are named here.
pub struct SieveEndpoint {
    /// The server host name.
    pub host: String,
    /// The port, 4190 being the only one RFC 5804 registers.
    pub port: u16,
    /// Whether the endpoint is reached by upgrading a cleartext
    /// connection rather than by handshaking straight away.
    pub starttls: bool,
}

/// Prompts once for an email address, a server URL or a bare domain,
/// builds the account from it and tests it, then hands it back beside
/// the name it suggests.
///
/// Every prompt renders on stderr, so a redirected stdout carries the
/// generated document alone.
pub fn run() -> Result<(String, AccountConfig)> {
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

    // NOTE: test every block before handing the account back, exactly
    // like himalaya, so a bad credential or endpoint fails here rather
    // than landing in a configuration that cannot connect. One resolver
    // for the whole account, so a credential command both blocks name is
    // spawned once.
    let mut secrets = SecretResolver::new();

    for protocol in account.protocols() {
        let server = account.server(protocol).expect("declared block exists");
        let connection = server.resolve_connection(protocol, &mut secrets)?;
        let spinner = Spinner::start(format!("Testing the {protocol} configuration"));

        if let Err(err) = session::test(protocol, connection) {
            spinner.failure(format!("The {protocol} configuration failed"));
            return Err(err);
        }

        spinner.success(format!("The {protocol} configuration is valid"));
    }

    Ok((name, account))
}

/// Derives the `[accounts.<name>]` table key suggested for `input`: the
/// first label of the email domain, of the URL host, or of a bare
/// domain. Only a suggestion, the user renames it in the printed
/// fragment.
fn default_account_name(input: &str) -> String {
    if let Some((_, domain)) = input.rsplit_once('@')
        && !input.contains("://")
    {
        return first_label(domain);
    }

    if let Ok(url) = Url::parse(input)
        && let Some(host) = url.host_str()
    {
        return first_label(host);
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
/// validates the scheme and host, derives the protocol and the STARTTLS
/// switch from the scheme, then prompts only for the SASL credentials.
///
/// A URL names one endpoint, so the account it builds declares the one
/// block that URL is about; a second protocol is a second `configure`
/// run, or a block written by hand.
fn build_url_account(url: Url, account_name: &str) -> Result<AccountConfig> {
    let scheme = url.scheme().to_ascii_lowercase();

    if url.host_str().is_none() {
        bail!("URL `{url}` is missing a host")
    }

    let protocol = Protocol::ALL
        .into_iter()
        .find(|protocol| protocol.schemes().contains(&scheme.as_str()));

    let Some(protocol) = protocol else {
        bail!("Unsupported URL scheme `{scheme}`, expects `imap(s)`, `smtp(s)` or `sieve(s)`");
    };

    let server = ServerConfig {
        server: url.to_string(),
        sock_file: None,
        tls: TlsConfig::default(),
        starttls: explicit_starttls(protocol, &scheme),
        allow_cleartext_auth: false,
        alpn: None,
        sasl: Some(prompt_sasl(account_name, None)?),
    };

    // NOTE: claiming the default is a property of the whole accounts
    // table, so it is decided when the account is saved, not when it is
    // built.
    Ok(match protocol {
        Protocol::Imap => AccountConfig {
            imap: Some(server),
            ..Default::default()
        },
        Protocol::Smtp => AccountConfig {
            smtp: Some(server),
            ..Default::default()
        },
        Protocol::Sieve => AccountConfig {
            sieve: Some(server),
            ..Default::default()
        },
    })
}

/// Builds an account from the discovered endpoints: runs the first-hit
/// discovery chain over the domain, then prompts once for the SASL
/// credentials both blocks share. Bails when nothing is discovered.
///
/// Whatever discovery returns is kept: an account speaks as many
/// protocols as it declares, so there is nothing to choose between and
/// no prompt asking which endpoint to throw away.
fn build_discovery_account(
    local_part: Option<&str>,
    domain: &str,
    account_name: &str,
) -> Result<AccountConfig> {
    let result = discover(local_part, domain);

    if result.is_empty() {
        bail!(
            "No configuration could be discovered for `{domain}`. \
             Try giving an `imap[s]://`, `smtp[s]://` or `sieve[s]://` URL instead."
        );
    }

    let DiscoveryResult { imap, smtp, sieve } = result;

    // NOTE: one provider, one credential, so the mechanism is prompted
    // once and every block carries it. A user needing two writes the
    // second by hand, which is what the sample documents.
    let login_default = local_part.map(|l| format!("{l}@{domain}"));
    let sasl = prompt_sasl(account_name, login_default.as_deref())?;

    Ok(AccountConfig {
        default: false,
        imap: imap.map(|endpoint| build_imap_server(endpoint, sasl.clone())),
        smtp: smtp.map(|endpoint| build_smtp_server(endpoint, sasl.clone())),
        sieve: build_sieve_server(sieve, sasl),
    })
}

/// Assembles the `sieve` block from a discovered endpoint.
#[cfg(feature = "sieve")]
fn build_sieve_server(sieve: Option<SieveEndpoint>, sasl: SaslConfig) -> Option<ServerConfig> {
    sieve.map(|endpoint| {
        build_server(
            Protocol::Sieve,
            endpoint.host,
            endpoint.port,
            endpoint.starttls,
            sasl,
        )
    })
}

/// A build without the `sieve` feature cannot serve a ManageSieve
/// session, so it generates no block promising one.
#[cfg(not(feature = "sieve"))]
fn build_sieve_server(_sieve: Option<SieveEndpoint>, _sasl: SaslConfig) -> Option<ServerConfig> {
    None
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

    if let Some(local) = local_part
        && let Some(result) = autoconfig::run_isp(local, domain)
            .map(|c| autoconfig::defaults(&c))
            .filter(|r| !r.is_empty())
    {
        return result;
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

/// Assembles the `imap` block from a discovered endpoint, deriving the
/// scheme and STARTTLS switch from its encryption.
fn build_imap_server(endpoint: WizardImapConfig, sasl: SaslConfig) -> ServerConfig {
    let starttls = matches!(endpoint.encryption, ImapEncryption::StartTls);
    build_server(Protocol::Imap, endpoint.host, endpoint.port, starttls, sasl)
}

/// Assembles the `smtp` block from a discovered endpoint, deriving the
/// scheme and STARTTLS switch from its encryption.
fn build_smtp_server(endpoint: WizardSmtpConfig, sasl: SaslConfig) -> ServerConfig {
    let starttls = matches!(endpoint.encryption, SmtpEncryption::StartTls);
    build_server(Protocol::Smtp, endpoint.host, endpoint.port, starttls, sasl)
}

/// Assembles one block, writing the scheme out rather than leaning on the
/// protocol default: a discovered endpoint says which encryption it
/// wants, and a generated configuration should say so too.
fn build_server(
    protocol: Protocol,
    host: String,
    port: u16,
    starttls: bool,
    sasl: SaslConfig,
) -> ServerConfig {
    let scheme = if starttls {
        protocol.cleartext_scheme()
    } else {
        protocol.tls_scheme()
    };

    ServerConfig {
        server: format!("{scheme}://{host}:{port}"),
        sock_file: None,
        tls: TlsConfig::default(),
        starttls: starttls_override(protocol, scheme, starttls),
        allow_cleartext_auth: false,
        alpn: None,
        sasl: Some(sasl),
    }
}

/// The `starttls` a generated block carries: [`None`] when the protocol
/// already answers it for that scheme, so the fragment stays free of
/// lines restating a default.
fn starttls_override(protocol: Protocol, scheme: &str, starttls: bool) -> Option<bool> {
    (starttls != protocol.default_starttls(scheme)).then_some(starttls)
}

/// The `starttls` a block built from a user-given URL carries.
///
/// The scheme is what the user chose, so the switch follows it: a
/// cleartext scheme is upgraded and an implicit-TLS one is not, unless
/// the protocol already says exactly that.
fn explicit_starttls(protocol: Protocol, scheme: &str) -> Option<bool> {
    starttls_override(protocol, scheme, scheme == protocol.cleartext_scheme())
}
