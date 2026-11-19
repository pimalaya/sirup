//! TOML configuration for the `sirup` CLI.
//!
//! [`Config`] is the whole config file: a sockets directory plus a table
//! of named [`AccountConfig`] entries. An account is a mailbox rather
//! than a session, so it declares one [`ServerConfig`] per protocol it
//! speaks, each carrying that protocol's own server URL, TLS profile,
//! STARTTLS switch, ALPN override and SASL mechanism. This is the shape
//! every other Pimalaya binary reads.
//!
//! [`parse_server`] validates a server URL against the protocol
//! whose block it was read from.
//!
//! Every path field is shell-expanded as it is deserialized, so no call
//! site can read one as written and no new field can forget to expand.

use std::{collections::HashMap, env::temp_dir, fs, path::PathBuf};

use anyhow::{Result, bail};
use io_sasl::{
    login::SaslLoginCreds, mechanism::Sasl, rfc4505::anonymous::SaslAnonymousCreds,
    rfc4616::plain::SaslPlainCreds, rfc5801::SaslGs2ChannelBinding, rfc5802::SaslScramCreds,
    rfc7628::oauthbearer::SaslOauthbearerCreds, xoauth2::SaslXoauth2Creds,
};
use log::warn;
use pimalaya_config::{
    secret::{Secret, SecretResolver},
    toml::{TomlConfig, opt_shell_expanded_path, shell_expanded_path},
};
use pimalaya_stream::tls::{Rustls, RustlsCrypto, Tls, TlsProvider, TlsServer};
use serde::Deserialize;
use url::Url;

use crate::protocol::Protocol;

/// The documented sample every field is described in, named by each
/// configuration failure as the way out.
pub const CONFIG_SAMPLE_URL: &str =
    "https://github.com/pimalaya/sirup/blob/master/config.sample.toml";

/// The whole TOML configuration file.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    /// Directory holding every per-account Unix socket. Defaults to the
    /// runtime dir, falling back to a temp dir when none is found.
    #[serde(
        default = "default_socks_dir",
        deserialize_with = "shell_expanded_path"
    )]
    pub socks_dir: PathBuf,
    /// The accounts, keyed by the name heading their `[accounts.<name>]`
    /// table.
    pub accounts: HashMap<String, AccountConfig>,
}

impl Config {
    /// Builds the socket path one protocol of an account is served on:
    /// `<socks_dir>/sirup/<name>-<protocol>.sock`.
    ///
    /// The protocol is part of the name because an account serves as many
    /// sockets as it declares blocks, and a `sock-file` in the block
    /// overrides the whole path.
    pub fn sock_path(&self, account_name: &str, protocol: Protocol) -> PathBuf {
        self.socks_dir
            .join(env!("CARGO_PKG_NAME"))
            .join(format!("{account_name}-{protocol}.sock"))
    }
}

impl TomlConfig for Config {
    type Account = AccountConfig;

    fn project_name() -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    fn sample_url() -> &'static str {
        CONFIG_SAMPLE_URL
    }

    fn accounts(&self) -> impl Iterator<Item = (&str, bool)> {
        self.accounts
            .iter()
            .map(|(name, account)| (name.as_str(), account.default))
    }

    fn take_named_account(&mut self, name: &str) -> Option<(String, Self::Account)> {
        self.accounts.remove_entry(name)
    }
}

/// One `[accounts.<name>]` block: a mailbox, and one server per protocol
/// it speaks.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AccountConfig {
    /// Whether this account is used when a command runs without `-a`.
    /// Exactly one account should set it.
    #[serde(default)]
    pub default: bool,
    /// The IMAP server, served on the account's `imap` socket.
    pub imap: Option<ServerConfig>,
    /// The SMTP submission server, served on the account's `smtp`
    /// socket.
    pub smtp: Option<ServerConfig>,
    /// The ManageSieve server, served on the account's `sieve` socket.
    pub sieve: Option<ServerConfig>,
}

impl AccountConfig {
    /// The server declared for `protocol`, `None` when the account
    /// declares no such block.
    pub fn server(&self, protocol: Protocol) -> Option<&ServerConfig> {
        match protocol {
            Protocol::Imap => self.imap.as_ref(),
            Protocol::Smtp => self.smtp.as_ref(),
            Protocol::Sieve => self.sieve.as_ref(),
        }
    }

    /// The protocols this account declares a server for, in the order it
    /// serves them.
    pub fn protocols(&self) -> Vec<Protocol> {
        Protocol::ALL
            .into_iter()
            .filter(|protocol| self.server(*protocol).is_some())
            .collect()
    }
}

/// One protocol's server: where it is, how the connection is secured and
/// who it authenticates as.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServerConfig {
    /// Server address, a bare authority or a full URL.
    ///
    /// A bare authority takes the protocol's implicit-TLS scheme
    /// (`imaps://`, `smtps://`). A full URL is used verbatim, the
    /// cleartext scheme being cleartext with an optional STARTTLS
    /// upgrade. A scheme the block's protocol does not speak is
    /// rejected.
    pub server: String,
    /// Override for this protocol's socket path, replacing the
    /// `<socks_dir>/sirup/<account>-<protocol>.sock` default.
    #[serde(default, deserialize_with = "opt_shell_expanded_path")]
    pub sock_file: Option<PathBuf>,
    /// TLS provider and trust settings for the handshake.
    #[serde(default)]
    pub tls: TlsConfig,
    /// Whether to upgrade a plaintext connection with STARTTLS. Only
    /// valid with the cleartext scheme (`imap://`, `smtp://`,
    /// `sieve://`).
    ///
    /// Omitted, it follows the protocol: on ManageSieve a cleartext
    /// server is upgraded, RFC 5804 defining that as the way to reach
    /// TLS on its single port; IMAP and SMTP have an implicit-TLS
    /// scheme of their own, so theirs stays off until asked for.
    pub starttls: Option<bool>,
    /// Whether a SASL mechanism disclosing a reusable credential may
    /// run over a connection that is still in the clear.
    ///
    /// PLAIN, LOGIN, OAUTHBEARER and XOAUTH2 hand a passive observer
    /// something it can replay. Only the ManageSieve session enforces
    /// this today, refusing them by default as RFC 5804 section 5 asks;
    /// the IMAP and SMTP sessions send what they are configured to
    /// send. The key exists only in builds with the `sieve` feature.
    #[cfg(feature = "sieve")]
    #[serde(default)]
    pub allow_cleartext_auth: bool,
    /// ALPN protocol identifiers offered during the TLS handshake.
    /// `None` (field omitted) takes the protocol's own: `["imap"]` for
    /// IMAP, `["smtp"]` for SMTP, none at all for ManageSieve, which
    /// registers no identifier. `Some([])` disables ALPN entirely;
    /// `Some(["x"])` overrides with a custom list. Only relevant for the
    /// rustls provider; `native-tls` ignores ALPN.
    pub alpn: Option<Vec<String>>,
    /// The SASL mechanism and its credentials. Omit to skip
    /// authentication entirely.
    pub sasl: Option<SaslConfig>,
}

/// Everything opening one upstream session needs, resolved from a
/// [`ServerConfig`] and the protocol whose block it came from.
///
/// A struct rather than the tuple it grew out of: five values threaded
/// through two call sites read better named than positioned.
pub struct Connection {
    /// The server URL, its scheme filled in when the block left it out.
    pub url: Url,
    /// The TLS handle, carrying the resolved ALPN list.
    pub tls: Tls,
    /// Whether to upgrade the connection with STARTTLS.
    pub starttls: bool,
    /// Whether a credential-disclosing mechanism may run in the clear.
    #[cfg(feature = "sieve")]
    pub allow_cleartext_auth: bool,
    /// The resolved SASL credentials, `None` to skip authentication.
    pub sasl: Option<Sasl>,
}

impl ServerConfig {
    /// Resolves everything opening the session needs.
    ///
    /// A missing scheme, a missing ALPN, a missing STARTTLS switch and a
    /// portless URL all take the protocol's own defaults. Secrets go
    /// through `secrets` rather than resolving themselves, so one
    /// credential command named by several blocks of an account is
    /// spawned once.
    pub fn resolve_connection(
        &self,
        protocol: Protocol,
        secrets: &mut SecretResolver,
    ) -> Result<Connection> {
        let url = parse_server(&self.server, protocol)?;
        let scheme = url.scheme();

        // NOTE: an explicit empty vec disables ALPN, a non-empty one
        // overrides the protocol's default.
        let alpn = self.alpn.clone().unwrap_or_else(|| protocol.default_alpn());
        let tls = self.tls.clone().into_tls(alpn);

        // NOTE: gating the SASL config on port_or_known_default() would
        // silently drop it for a portless URL like
        // `imaps://mail.example.com`, opening an unauthenticated session.
        // Host and port only feed OAUTHBEARER; the other mechanisms
        // ignore them.
        let sasl = self
            .sasl
            .clone()
            .map(|cfg| {
                let host = url.host_str().unwrap_or_default();
                let port = url.port().unwrap_or_else(|| protocol.default_port(scheme));
                cfg.try_into_sasl(host, port, secrets)
            })
            .transpose()?;

        Ok(Connection {
            starttls: self
                .starttls
                .unwrap_or_else(|| protocol.default_starttls(scheme)),
            #[cfg(feature = "sieve")]
            allow_cleartext_auth: self.allow_cleartext_auth,
            url,
            tls,
            sasl,
        })
    }
}

/// Parses a block's `server` string into a [`Url`], validating the
/// scheme against the protocol the block was read from.
///
/// A bare authority takes the protocol's implicit-TLS scheme, a full URL
/// is used verbatim, and a scheme the protocol does not speak is
/// rejected naming the ones it does. Mirrors himalaya's `parse_server`,
/// the block knowing its protocol being what makes the scheme optional.
pub fn parse_server(server: &str, protocol: Protocol) -> Result<Url> {
    let url = if server.contains("://") {
        Url::parse(server)?
    } else {
        Url::parse(&format!("{}://{server}", protocol.default_scheme()))?
    };

    let scheme = url.scheme();

    if !protocol.schemes().contains(&scheme) {
        bail!(
            "Invalid `{protocol}.server` scheme `{scheme}`, expected one of {:?}",
            protocol.schemes(),
        );
    }

    if url.host_str().is_none() {
        bail!("Server `{server}` is missing a host");
    }

    Ok(url)
}

/// TLS provider and trust settings for an account.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TlsConfig {
    /// TLS backend to use; defaults to the first available at runtime.
    pub provider: Option<TlsProviderConfig>,
    /// Rustls-specific settings, ignored by the native-tls provider.
    #[serde(default)]
    pub rustls: RustlsConfig,
    /// Extra root certificate to trust, PEM-encoded.
    #[serde(default, deserialize_with = "opt_shell_expanded_path")]
    pub cert: Option<PathBuf>,
}

/// The selectable TLS backend.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum TlsProviderConfig {
    /// The Rustls pure-Rust stack.
    Rustls,
    /// The platform-native TLS stack.
    NativeTls,
}

/// Rustls-specific settings.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RustlsConfig {
    /// Crypto backend for Rustls; defaults to the first available.
    pub crypto: Option<RustlsCryptoConfig>,
}

/// The selectable Rustls crypto backend.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum RustlsCryptoConfig {
    /// The aws-lc-rs backend.
    Aws,
    /// The ring backend.
    Ring,
}

impl TlsConfig {
    /// Builds the runtime [`Tls`] handle the connect helpers expect.
    /// `alpn` is the protocol-level ALPN list (e.g. `["imap"]`,
    /// `["smtp"]`); pass an empty vec to skip ALPN. The TOML schema
    /// never exposes `tls.rustls.alpn` directly: the account-level
    /// `alpn` field is folded in here.
    pub fn into_tls(self, alpn: Vec<String>) -> Tls {
        Tls {
            provider: self.provider.map(|p| match p {
                TlsProviderConfig::Rustls => TlsProvider::Rustls,
                TlsProviderConfig::NativeTls => TlsProvider::NativeTls,
            }),
            rustls: Rustls {
                crypto: self.rustls.crypto.map(|c| match c {
                    RustlsCryptoConfig::Aws => RustlsCrypto::Aws,
                    RustlsCryptoConfig::Ring => RustlsCrypto::Ring,
                }),
                alpn,
            },
            server: self.cert.map(|cert| TlsServer { cert }),
            client: None,
        }
    }
}

/// Per-account SASL configuration.
///
/// Exactly one mechanism is selected per account; each variant carries
/// the credentials for that mechanism. Maps 1:1 to the variants of
/// [`io_sasl::mechanism::Sasl`].
///
/// `scram-sha-256` only works at runtime when the `scram` cargo feature
/// is enabled (it propagates to `io-imap`/`io-smtp`); otherwise the
/// upstream client returns `ScramSha256NotEnabled`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum SaslConfig {
    /// The ANONYMOUS mechanism (RFC 4505), carrying only a trace token.
    Anonymous(SaslAnonymousConfig),
    /// The obsolete LOGIN mechanism.
    Login(SaslLoginConfig),
    /// The PLAIN mechanism (RFC 4616).
    Plain(SaslPlainConfig),
    /// The OAUTHBEARER mechanism (RFC 7628).
    Oauthbearer(SaslOauthbearerConfig),
    /// Google's pre-standard XOAUTH2 mechanism.
    Xoauth2(SaslXoauth2Config),
    /// The SCRAM-SHA-256 mechanism (RFC 7677), needing the `scram`
    /// feature.
    #[serde(rename = "scram-sha-256")]
    ScramSha256(SaslScramSha256Config),
}

/// Credentials for the ANONYMOUS mechanism.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslAnonymousConfig {
    /// Optional trace token sent to the server.
    pub message: Option<String>,
}

/// Credentials for the LOGIN mechanism.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslLoginConfig {
    /// The login name.
    pub username: String,
    /// The password secret.
    pub password: Secret,
}

/// Credentials for the PLAIN mechanism.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslPlainConfig {
    /// Optional authorization identity to act as.
    pub authzid: Option<String>,
    /// Authentication identity (the login), also accepted as `username`.
    #[serde(alias = "username")]
    pub authcid: String,
    /// The password secret, also accepted as `password`.
    #[serde(alias = "password")]
    pub passwd: Secret,
}

/// Credentials for the OAUTHBEARER mechanism.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslOauthbearerConfig {
    /// The account username.
    pub username: String,
    /// The OAuth 2.0 bearer token secret.
    pub token: Secret,
}

/// Credentials for the XOAUTH2 mechanism.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslXoauth2Config {
    /// The account username.
    pub username: String,
    /// The OAuth 2.0 access token secret.
    pub token: Secret,
}

/// Credentials for the SCRAM-SHA-256 mechanism.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslScramSha256Config {
    /// The login name.
    pub username: String,
    /// The password secret.
    pub password: Secret,
}

impl SaslConfig {
    /// Resolves the config into a runtime [`Sasl`], reading any
    /// command-backed secret through `secrets`. `host` and `port` seed
    /// the OAUTHBEARER GS2 header; the other mechanisms ignore them.
    ///
    /// The resolver is what keeps an account naming one credential
    /// command from both its blocks to a single spawn, which for a `pass`
    /// or `gpg` entry is a single key unlock.
    pub fn try_into_sasl(
        self,
        host: impl ToString,
        port: u16,
        secrets: &mut SecretResolver,
    ) -> Result<Sasl> {
        Ok(match self {
            SaslConfig::Anonymous(c) => Sasl::Anonymous(SaslAnonymousCreds { message: c.message }),
            SaslConfig::Login(c) => Sasl::Login(SaslLoginCreds {
                username: c.username,
                password: secrets.resolve(c.password)?,
            }),
            SaslConfig::Plain(c) => Sasl::Plain(SaslPlainCreds {
                authzid: c.authzid,
                authcid: c.authcid,
                passwd: secrets.resolve(c.passwd)?,
            }),
            SaslConfig::Oauthbearer(c) => Sasl::Oauthbearer(SaslOauthbearerCreds {
                username: c.username,
                host: host.to_string(),
                port,
                token: secrets.resolve(c.token)?,
            }),
            SaslConfig::Xoauth2(c) => Sasl::Xoauth2(SaslXoauth2Creds {
                username: c.username,
                token: secrets.resolve(c.token)?,
            }),
            // NOTE: an empty nonce means draw one for me: the client
            // fills it in, an I/O-free coroutine having no randomness of
            // its own.
            SaslConfig::ScramSha256(c) => Sasl::ScramSha256(SaslScramCreds {
                username: c.username,
                password: secrets.resolve(c.password)?,
                nonce: Vec::new(),
                channel_binding: SaslGs2ChannelBinding::Unsupported,
            }),
        })
    }
}

fn default_socks_dir() -> PathBuf {
    if let Some(path) = dirs::runtime_dir() {
        return path;
    }

    let path = temp_dir().join(format!("service-{}", env!("CARGO_PKG_NAME")));
    let p = path.display();

    warn!("runtime dir not found, falling back to {p}");

    if let Err(err) = fs::create_dir_all(&path) {
        warn!("cannot create dir {p} ({err}), assuming it already exists");
    }

    path
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    /// An account speaking every protocol, naming every path field the
    /// schema has, each written with a leading tilde.
    fn config() -> Config {
        toml::from_str(
            r#"
socks-dir = "~/run"

[accounts.work]
default = true
imap.server = "mail.example.com"
imap.sock-file = "~/run/work-imap.sock"
imap.tls.cert = "~/certs/ca.pem"
smtp.server = "smtp://mail.example.com:587"
smtp.starttls = true
sieve.server = "mail.example.com"
"#,
        )
        .expect("parse the config")
    }

    #[test]
    fn every_path_is_expanded_at_deserialize() {
        let home = env::var("HOME").expect("HOME must be set");
        let config = config();
        let imap = config.accounts["work"]
            .imap
            .as_ref()
            .expect("an imap block");

        assert_eq!(config.socks_dir, PathBuf::from(format!("{home}/run")));
        assert_eq!(
            imap.sock_file,
            Some(PathBuf::from(format!("{home}/run/work-imap.sock")))
        );
        assert_eq!(
            imap.tls.cert,
            Some(PathBuf::from(format!("{home}/certs/ca.pem")))
        );
    }

    #[test]
    fn an_account_declares_the_protocols_it_speaks() {
        let config = config();

        assert_eq!(
            config.accounts["work"].protocols(),
            [Protocol::Imap, Protocol::Smtp, Protocol::Sieve]
        );
        assert_eq!(
            config.sock_path("work", Protocol::Smtp),
            PathBuf::from(format!(
                "{}/sirup/work-smtp.sock",
                config.socks_dir.display()
            ))
        );
    }

    #[test]
    fn a_bare_authority_takes_the_protocol_defaults() {
        let config = config();
        let imap = config.accounts["work"]
            .imap
            .as_ref()
            .expect("an imap block");
        let mut secrets = SecretResolver::new();
        let connection = imap
            .resolve_connection(Protocol::Imap, &mut secrets)
            .expect("resolve the connection");

        assert_eq!(connection.url.scheme(), "imaps");
        assert_eq!(connection.url.port(), None);
        assert_eq!(Protocol::Imap.default_port(connection.url.scheme()), 993);
        assert_eq!(connection.tls.rustls.alpn, ["imap"]);
        assert!(!connection.starttls);
        assert!(connection.sasl.is_none());
    }

    #[cfg(feature = "sieve")]
    #[test]
    fn a_bare_managesieve_authority_is_upgraded_rather_than_encrypted() {
        // NOTE: RFC 5804 registers one port and reaches TLS on it
        // through STARTTLS, so the ManageSieve default is the cleartext
        // scheme plus the upgrade where the other two take implicit TLS.
        let config = config();
        let sieve = config.accounts["work"]
            .sieve
            .as_ref()
            .expect("a sieve block");
        let mut secrets = SecretResolver::new();
        let connection = sieve
            .resolve_connection(Protocol::Sieve, &mut secrets)
            .expect("resolve the connection");

        assert_eq!(connection.url.scheme(), "sieve");
        assert_eq!(Protocol::Sieve.default_port(connection.url.scheme()), 4190);
        assert!(connection.starttls);
        assert!(connection.tls.rustls.alpn.is_empty());
        assert!(!connection.allow_cleartext_auth);
    }

    #[test]
    fn a_scheme_of_another_protocol_is_rejected() {
        let err = parse_server("imaps://mail.example.com", Protocol::Smtp)
            .expect_err("an imaps URL is not an smtp server");

        assert!(err.to_string().contains("smtp.server"));
    }
}
