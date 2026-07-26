//! TOML configuration for the `sirup` CLI.
//!
//! [`Config`] is the whole config file: a sockets directory plus a table
//! of named [`AccountConfig`] entries, each carrying a server URL, a TLS
//! profile, a STARTTLS switch, an optional ALPN override and a single
//! SASL mechanism. The types derive both `Deserialize` (loading a config)
//! and `Serialize` (the wizard printing a ready-to-save fragment), so the
//! `skip_serializing_if` predicates keep defaulted fields out of that
//! fragment. [`parse_server`] validates a server URL into its protocol.

use std::{collections::HashMap, env::temp_dir, fs, path::PathBuf};

use anyhow::{Result, bail};
use log::warn;
use pimalaya_config::{secret::Secret, toml::TomlConfig};
use pimalaya_stream::{
    sasl::{
        Sasl, SaslAnonymous, SaslLogin, SaslOauthbearer, SaslPlain, SaslScramSha256, SaslXoauth2,
    },
    tls::{Rustls, RustlsCrypto, Tls, TlsProvider},
};
use serde::{Deserialize, Serialize};
use url::Url;

/// The whole TOML configuration file.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    /// Directory holding every per-account Unix socket. Defaults to the
    /// runtime dir, falling back to a temp dir when none is found.
    #[serde(default = "default_socks_dir")]
    pub socks_dir: PathBuf,
    /// The accounts, keyed by the name heading their `[accounts.<name>]`
    /// table.
    pub accounts: HashMap<String, AccountConfig>,
}

impl Config {
    /// Builds the socket path for an account: `<socks_dir>/sirup/<name>.sock`.
    pub fn sock_path(&self, account_name: &str) -> PathBuf {
        self.socks_dir
            .join(env!("CARGO_PKG_NAME"))
            .join(format!("{account_name}.sock"))
    }
}

impl TomlConfig for Config {
    type Account = AccountConfig;

    fn project_name() -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    fn take_named_account(&mut self, name: &str) -> Option<(String, Self::Account)> {
        self.accounts.remove_entry(name)
    }

    fn take_default_account(&mut self) -> Option<(String, Self::Account)> {
        let name = self
            .accounts
            .iter()
            .find_map(|(name, account)| account.default.then(|| name.clone()))?;

        self.take_named_account(&name)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AccountConfig {
    /// Whether this account is used when a command runs without `-a`.
    /// Exactly one account should set it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub default: bool,
    /// Override for this account's socket path, replacing the
    /// `<socks_dir>/sirup/<name>.sock` default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sock_file: Option<PathBuf>,
    /// Backend server address as a full `imap://`, `imaps://`,
    /// `smtp://` or `smtps://` URL. The scheme picks the protocol
    /// (IMAP vs SMTP) and its `s` suffix selects implicit TLS.
    ///
    /// Mirrors himalaya's `imap.server` / `smtp.server`, but the
    /// scheme is mandatory here: a single sirup account serves either
    /// protocol, so there is no per-protocol default scheme to fall
    /// back to for a bare authority.
    pub server: String,
    /// TLS provider and trust settings for the handshake.
    #[serde(default, skip_serializing_if = "is_default_tls")]
    pub tls: TlsConfig,
    /// Whether to upgrade a plaintext connection with STARTTLS. Only
    /// valid with the non-`s` schemes (`imap://`, `smtp://`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub starttls: bool,
    /// ALPN protocol identifiers offered during the TLS handshake.
    /// `None` (field omitted) infers a sensible default from the URL
    /// scheme: `["imap"]` for `imap[s]://`, `["smtp"]` for
    /// `smtp[s]://`. `Some([])` disables ALPN entirely; `Some(["x"])`
    /// overrides with a custom list. Only relevant for the rustls
    /// provider; `native-tls` ignores ALPN.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpn: Option<Vec<String>>,
    /// The SASL mechanism and its credentials. Omit to skip
    /// authentication entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sasl: Option<SaslConfig>,
}

/// `skip_serializing_if` predicate for a `bool` field: skips it when
/// `false`, so a wizard-generated fragment omits the off switches. The
/// wizard is the only serializer of these types.
fn is_false(value: &bool) -> bool {
    !*value
}

/// `skip_serializing_if` predicate for the [`TlsConfig`] field: skips it
/// when left at its default, so the fragment carries no empty TLS block.
fn is_default_tls(tls: &TlsConfig) -> bool {
    *tls == TlsConfig::default()
}

/// Parses an account `server` string into a [`Url`], validating the
/// scheme.
///
/// The scheme is mandatory and selects the protocol: `imap`/`imaps`
/// for IMAP, `smtp`/`smtps` for SMTP, the `s` suffix picking implicit
/// TLS. Unlike himalaya's per-protocol `parse_server`, a bare
/// authority is rejected: a single sirup account serves either
/// protocol, so there is no single scheme to default to.
pub fn parse_server(server: &str) -> Result<Url> {
    let url = Url::parse(server)?;
    let scheme = url.scheme();

    if !matches!(scheme, "imap" | "imaps" | "smtp" | "smtps") {
        bail!("Invalid server scheme `{scheme}`, expects `imap(s)` or `smtp(s)`");
    }

    if url.host_str().is_none() {
        bail!("Server `{server}` is missing a host");
    }

    Ok(url)
}

/// TLS provider and trust settings for an account.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TlsConfig {
    /// TLS backend to use; defaults to the first available at runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<TlsProviderConfig>,
    /// Rustls-specific settings, ignored by the native-tls provider.
    #[serde(default)]
    pub rustls: RustlsConfig,
    /// Extra root certificate to trust, PEM-encoded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert: Option<PathBuf>,
}

/// The selectable TLS backend.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum TlsProviderConfig {
    /// The Rustls pure-Rust stack.
    Rustls,
    /// The platform-native TLS stack.
    NativeTls,
}

/// Rustls-specific settings.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RustlsConfig {
    /// Crypto backend for Rustls; defaults to the first available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crypto: Option<RustlsCryptoConfig>,
}

/// The selectable Rustls crypto backend.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
            cert: self.cert,
        }
    }
}

/// Per-account SASL configuration.
///
/// Exactly one mechanism is selected per account; each variant carries
/// the credentials for that mechanism. Maps 1:1 to the variants of
/// [`pimalaya_stream::sasl::Sasl`].
///
/// `scram-sha-256` only works at runtime when the `scram` cargo feature
/// is enabled (it propagates to `io-imap`/`io-smtp`); otherwise the
/// upstream client returns `ScramSha256NotEnabled`.
#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslAnonymousConfig {
    /// Optional trace token sent to the server.
    pub message: Option<String>,
}

/// Credentials for the LOGIN mechanism.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslLoginConfig {
    /// The login name.
    pub username: String,
    /// The password secret.
    pub password: Secret,
}

/// Credentials for the PLAIN mechanism.
#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslOauthbearerConfig {
    /// The account username.
    pub username: String,
    /// The OAuth 2.0 bearer token secret.
    pub token: Secret,
}

/// Credentials for the XOAUTH2 mechanism.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslXoauth2Config {
    /// The account username.
    pub username: String,
    /// The OAuth 2.0 access token secret.
    pub token: Secret,
}

/// Credentials for the SCRAM-SHA-256 mechanism.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SaslScramSha256Config {
    /// The login name.
    pub username: String,
    /// The password secret.
    pub password: Secret,
}

impl SaslConfig {
    /// Resolves the config into a runtime [`Sasl`], reading any
    /// command-backed secret. `host` and `port` seed the OAUTHBEARER GS2
    /// header; the other mechanisms ignore them.
    pub fn try_into_sasl(self, host: impl ToString, port: u16) -> Result<Sasl> {
        Ok(match self {
            SaslConfig::Anonymous(c) => Sasl::Anonymous(SaslAnonymous { message: c.message }),
            SaslConfig::Login(c) => Sasl::Login(SaslLogin {
                username: c.username,
                password: c.password.get()?,
            }),
            SaslConfig::Plain(c) => Sasl::Plain(SaslPlain {
                authzid: c.authzid,
                authcid: c.authcid,
                passwd: c.passwd.get()?,
            }),
            SaslConfig::Oauthbearer(c) => Sasl::Oauthbearer(SaslOauthbearer {
                username: c.username,
                host: host.to_string(),
                port,
                token: c.token.get()?,
            }),
            SaslConfig::Xoauth2(c) => Sasl::Xoauth2(SaslXoauth2 {
                username: c.username,
                token: c.token.get()?,
            }),
            SaslConfig::ScramSha256(c) => Sasl::ScramSha256(SaslScramSha256 {
                username: c.username,
                password: c.password.get()?,
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
