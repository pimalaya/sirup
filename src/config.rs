//! TOML configuration for the `sirup` CLI.
//!
//! [`Config`] is the whole config file: a sockets directory plus a table
//! of named [`AccountConfig`] entries, each carrying a server URL, a TLS
//! profile, a STARTTLS switch, an optional ALPN override and a single
//! SASL mechanism. The types derive both `Deserialize` (loading a config)
//! and `Serialize` (the wizard rendering a ready-to-save fragment), so the
//! `skip_serializing_if` predicates keep defaulted fields out of that
//! fragment. [`parse_server`] validates a server URL into its protocol.
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
    secret::Secret,
    toml::{TomlConfig, shell_expanded_path},
};
use pimalaya_stream::tls::{Rustls, RustlsCrypto, Tls, TlsProvider};
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

/// The documented sample every field is described in, named by the
/// welcome and by each configuration failure as the way out that needs
/// no wizard.
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
    #[serde(default, deserialize_with = "opt_shell_expanded_path")]
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

impl AccountConfig {
    /// Resolves the runtime connection parameters: the server URL, the
    /// TLS handle, the STARTTLS switch and the SASL credentials.
    ///
    /// A missing ALPN and a portless URL both infer from the server
    /// scheme.
    pub fn resolve_connection(&self) -> Result<(Url, Tls, bool, Option<Sasl>)> {
        let server = parse_server(&self.server)?;
        let scheme = server.scheme();

        // NOTE: a missing alpn infers from the server scheme (["imap"]
        // for imap[s]://, ["smtp"] for smtp[s]://); an explicit empty vec
        // disables ALPN, a non-empty one overrides the default.
        let alpn = self.alpn.clone().unwrap_or_else(|| default_alpn(scheme));
        let tls = self.tls.clone().into_tls(alpn);

        // NOTE: the url crate only knows default ports for web schemes,
        // so imap(s)/smtp(s) need an explicit fallback. Gating the SASL
        // config on port_or_known_default() would silently drop it for a
        // portless URL like `imaps://mail.example.com`, opening an
        // unauthenticated session. Host and port only feed OAUTHBEARER;
        // the other mechanisms ignore them.
        let sasl = self
            .sasl
            .clone()
            .map(|cfg| {
                let host = server.host_str().unwrap_or_default();
                let port = server.port().unwrap_or_else(|| default_port(scheme));
                cfg.try_into_sasl(host, port)
            })
            .transpose()?;

        Ok((server, tls, self.starttls, sasl))
    }

    /// Renders the account as an `[accounts.<name>]` TOML table, the
    /// document the wizard saves, appends or prints.
    ///
    /// Borrowed rather than moved into a [`Config`], which would mean
    /// cloning the account to render it, and the endpoint is lifted to
    /// the top of the table: serialized alphabetically it would sit
    /// under the credentials that qualify it.
    pub fn render(&self, name: &str) -> Result<String> {
        #[derive(Serialize)]
        struct AccountDocument<'a> {
            accounts: HashMap<&'a str, &'a AccountConfig>,
        }

        let document = AccountDocument {
            accounts: HashMap::from([(name, self)]),
        };
        let rendered = pimalaya_config::toml::to_string(&document)?;

        let Some((header, body)) = rendered.split_once('\n') else {
            return Ok(rendered);
        };

        let mut lines: Vec<&str> = body
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        lines.sort_by_key(|line| !line.starts_with("server "));

        let mut document = format!("{header}\n");

        for line in lines {
            document.push_str(line);
            document.push('\n');
        }

        Ok(document)
    }
}

/// `skip_serializing_if` predicate for a `bool` field: skips it when
/// `false`, so a wizard-generated fragment omits the off switches. The
/// wizard is the only serializer of these types.
fn is_false(value: &bool) -> bool {
    !*value
}

// NOTE: these mirror io-imap's and io-smtp's own `default_alpn()` and
// `default_port()`, kept local so the schema depends on no backend crate
// and resolves the same under any feature subset.

/// Default ALPN identifiers for a server scheme: `["imap"]` for
/// `imap[s]://` <sup>[rfc7595]</sup>, `["smtp"]` for `smtp[s]://`.
///
/// [rfc7595]: https://www.iana.org/go/rfc7595
fn default_alpn(scheme: &str) -> Vec<String> {
    match scheme {
        "imap" | "imaps" => vec![String::from("imap")],
        "smtp" | "smtps" => vec![String::from("smtp")],
        _ => Vec::new(),
    }
}

/// Default port for a server scheme: 143 and 993 for IMAP
/// <sup>[rfc3501]</sup>, 25 and 465 for SMTP <sup>[rfc5321]</sup>.
///
/// [rfc3501]: https://www.iana.org/go/rfc3501
/// [rfc5321]: https://www.iana.org/go/rfc5321
fn default_port(scheme: &str) -> u16 {
    match scheme {
        "imap" => 143,
        "imaps" => 993,
        "smtp" => 25,
        "smtps" => 465,
        _ => 0,
    }
}

/// Deserializes an optional path field, expanding it exactly as
/// [`shell_expanded_path`] expands a required one.
///
/// pimalaya-config carries no optional twin yet, so the wrapper lives
/// here until it does.
fn opt_shell_expanded_path<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<PathBuf>, D::Error> {
    shell_expanded_path(deserializer).map(Some)
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
    #[serde(default, deserialize_with = "opt_shell_expanded_path")]
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
/// [`io_sasl::mechanism::Sasl`].
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
            SaslConfig::Anonymous(c) => Sasl::Anonymous(SaslAnonymousCreds { message: c.message }),
            SaslConfig::Login(c) => Sasl::Login(SaslLoginCreds {
                username: c.username,
                password: c.password.get()?,
            }),
            SaslConfig::Plain(c) => Sasl::Plain(SaslPlainCreds {
                authzid: c.authzid,
                authcid: c.authcid,
                passwd: c.passwd.get()?,
            }),
            SaslConfig::Oauthbearer(c) => Sasl::Oauthbearer(SaslOauthbearerCreds {
                username: c.username,
                host: host.to_string(),
                port,
                token: c.token.get()?,
            }),
            SaslConfig::Xoauth2(c) => Sasl::Xoauth2(SaslXoauth2Creds {
                username: c.username,
                token: c.token.get()?,
            }),
            // NOTE: an empty nonce means draw one for me: the client
            // fills it in, an I/O-free coroutine having no randomness of
            // its own.
            SaslConfig::ScramSha256(c) => Sasl::ScramSha256(SaslScramCreds {
                username: c.username,
                password: c.password.get()?,
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

    /// A configuration naming every path field it has, each written with
    /// a leading tilde.
    fn config() -> Config {
        toml::from_str(
            r#"
socks-dir = "~/run"

[accounts.work]
server = "imaps://mail.example.com"
sock-file = "~/run/work.sock"
tls.cert = "~/certs/ca.pem"
"#,
        )
        .expect("parse the config")
    }

    #[test]
    fn every_path_is_expanded_at_deserialize() {
        let home = env::var("HOME").expect("HOME must be set");
        let config = config();
        let account = &config.accounts["work"];

        assert_eq!(config.socks_dir, PathBuf::from(format!("{home}/run")));
        assert_eq!(
            account.sock_file,
            Some(PathBuf::from(format!("{home}/run/work.sock")))
        );
        assert_eq!(
            account.tls.cert,
            Some(PathBuf::from(format!("{home}/certs/ca.pem")))
        );
    }

    #[test]
    fn a_portless_url_infers_its_alpn_and_port() {
        let account = &config().accounts["work"];
        let (server, tls, starttls, sasl) = account
            .resolve_connection()
            .expect("resolve the connection");

        assert_eq!(server.port(), None);
        assert_eq!(default_port(server.scheme()), 993);
        assert_eq!(tls.rustls.alpn, ["imap"]);
        assert!(!starttls);
        assert!(sasl.is_none());
    }
}
