// This file is part of Sirup, a CLI to spawn pre-authenticated IMAP/SMTP
// sessions and expose them via Unix sockets.
//
// Copyright (C) 2026  soywod <pimalaya.org@posteo.net>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

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
//! 4. Prompt the SASL mechanism plus only the fields it needs.
//! 5. Return the built [`AccountConfig`]; nothing is persisted.

use anyhow::{Result, bail};
use pimalaya_cli::{
    prompt,
    wizard::{
        imap::{Encryption as ImapEncryption, WizardImapConfig},
        smtp::{Encryption as SmtpEncryption, WizardSmtpConfig},
    },
};
use pimalaya_config::secret::Secret;
use pimalaya_stream::tls::Tls;
use secrecy::SecretString;
use url::Url;

use crate::{
    config::{
        AccountConfig, SaslAnonymousConfig, SaslConfig, SaslLoginConfig, SaslOauthbearerConfig,
        SaslPlainConfig, SaslScramSha256Config, SaslXoauth2Config, TlsConfig,
    },
    wizard::{autoconfig, pacc, srv},
};

const DEFAULT_RESOLVER: &str = "tcp://1.1.1.1:53";

pub fn discovery_resolver() -> Url {
    DEFAULT_RESOLVER
        .parse()
        .expect("DEFAULT_RESOLVER must be a valid URL")
}

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
    pub fn is_empty(&self) -> bool {
        self.imap.is_none() && self.smtp.is_none()
    }
}

pub fn run() -> Result<AccountConfig> {
    let input = prompt::text::<&str>("Email address, server URL or domain:", None)?;
    run_with_input(input.trim())
}

pub fn run_with_input(input: &str) -> Result<AccountConfig> {
    match classify(input)? {
        Input::Url(url) => build_url_account(url),
        Input::Domain(domain) => build_discovery_account(None, &domain),
        Input::Email { local, domain } => build_discovery_account(Some(&local), &domain),
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

// ── URL input ───────────────────────────────────────────────────────────────

fn build_url_account(url: Url) -> Result<AccountConfig> {
    let scheme = url.scheme().to_ascii_lowercase();
    if url.host_str().is_none() {
        bail!("URL `{url}` is missing a host")
    }

    match scheme.as_str() {
        // Direct IMAP/SMTP URL: use as-is, just prompt for SASL.
        "imap" | "imaps" | "smtp" | "smtps" => {
            let starttls = matches!(scheme.as_str(), "imap" | "smtp");
            let sasl = prompt_sasl(None)?;
            Ok(AccountConfig {
                default: true,
                sock_file: None,
                url,
                tls: TlsConfig::default(),
                alpn: None,
                starttls,
                sasl: Some(sasl),
            })
        }
        other => bail!("Unsupported URL scheme `{other}`, expects `imap(s)` or `smtp(s)`"),
    }
}

// ── Domain / email input (first-hit-wins discovery, no picker) ──────────────

fn build_discovery_account(local_part: Option<&str>, domain: &str) -> Result<AccountConfig> {
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
            let sasl = prompt_sasl(login_default.as_deref())?;
            Ok(build_imap_account(endpoint, sasl))
        }
        Protocol::Smtp => {
            let endpoint = smtp.expect("smtp endpoint must be present when chosen");
            let sasl = prompt_sasl(login_default.as_deref())?;
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

// ── SASL prompts ────────────────────────────────────────────────────────────

const SASL_MECHANISMS: [&str; 6] = [
    "PLAIN",
    "LOGIN",
    "XOAUTH2",
    "OAUTHBEARER",
    "SCRAM-SHA-256",
    "ANONYMOUS",
];

fn prompt_sasl(email: Option<&str>) -> Result<SaslConfig> {
    let mechanism = prompt::item("SASL mechanism:", SASL_MECHANISMS, Some("PLAIN"))?;

    Ok(match mechanism {
        "PLAIN" => SaslConfig::Plain(SaslPlainConfig {
            authzid: None,
            authcid: prompt::text("Login:", email)?,
            passwd: prompt_raw_secret("Password")?,
        }),
        "LOGIN" => SaslConfig::Login(SaslLoginConfig {
            username: prompt::text("Username:", email)?,
            password: prompt_raw_secret("Password")?,
        }),
        "XOAUTH2" => SaslConfig::Xoauth2(SaslXoauth2Config {
            username: prompt::text("Username:", email)?,
            token: prompt_raw_secret("Access token")?,
        }),
        "OAUTHBEARER" => SaslConfig::Oauthbearer(SaslOauthbearerConfig {
            username: prompt::text("Username:", email)?,
            token: prompt_raw_secret("Access token")?,
        }),
        "SCRAM-SHA-256" => SaslConfig::ScramSha256(SaslScramSha256Config {
            username: prompt::text("Username:", email)?,
            password: prompt_raw_secret("Password")?,
        }),
        "ANONYMOUS" => SaslConfig::Anonymous(SaslAnonymousConfig {
            message: prompt::some_text::<&str>("Anonymous message (optional):", None)?,
        }),
        _ => unreachable!(),
    })
}

fn prompt_raw_secret(label: &str) -> Result<Secret> {
    let raw = prompt::secret(format!("{label}:"))?;
    Ok(Secret::Raw(SecretString::from(raw)))
}

// ── Account assembly ────────────────────────────────────────────────────────

fn build_imap_account(endpoint: WizardImapConfig, sasl: SaslConfig) -> AccountConfig {
    let starttls = matches!(endpoint.encryption, ImapEncryption::StartTls);
    let scheme = if starttls { "imap" } else { "imaps" };
    let url = Url::parse(&format!("{scheme}://{}:{}", endpoint.host, endpoint.port))
        .expect("imap URL must parse");

    AccountConfig {
        default: true,
        sock_file: None,
        url,
        tls: TlsConfig::default(),
        alpn: None,
        starttls,
        sasl: Some(sasl),
    }
}

fn build_smtp_account(endpoint: WizardSmtpConfig, sasl: SaslConfig) -> AccountConfig {
    let starttls = matches!(endpoint.encryption, SmtpEncryption::StartTls);
    let scheme = if starttls { "smtp" } else { "smtps" };
    let url = Url::parse(&format!("{scheme}://{}:{}", endpoint.host, endpoint.port))
        .expect("smtp URL must parse");

    AccountConfig {
        default: true,
        sock_file: None,
        url,
        tls: TlsConfig::default(),
        alpn: None,
        starttls,
        sasl: Some(sasl),
    }
}
