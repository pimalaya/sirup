//! Mozilla Thunderbird Autoconfiguration probes used by the wizard.
//! Three independent probes (ISP main, ISP fallback, Thunderbird
//! ISPDB) live behind their own `run_*` functions; `defaults` converts
//! a successful [`DiscoveryAutoconfig`] into the IMAP/SMTP-only
//! [`DiscoveryResult`] shape consumed by the discovery chain.

use io_pim_discovery::autoconfig::{
    client::{DiscoveryAutoconfigClientStd, DiscoveryAutoconfigClientStdError},
    config::{DiscoveryAutoconfig, DiscoverySecurityType, DiscoveryServer, DiscoveryServerType},
};
use log::trace;
use pimalaya_cli::{
    spinner::Spinner,
    wizard::{
        imap::{Encryption as ImapEncryption, ImapAuth, ImapSecret, WizardImapConfig},
        smtp::{Encryption as SmtpEncryption, SmtpAuth, SmtpSecret, WizardSmtpConfig},
    },
};

use crate::wizard::discover::{DiscoveryResult, discovery_resolver, discovery_tls};

/// Probes the ISP-hosted autoconfig URL, keyed by the full address.
pub fn run_isp(local_part: &str, domain: &str) -> Option<DiscoveryAutoconfig> {
    run_probe("Autoconfig ISP main URL", domain, |client| {
        client.isp(local_part, domain, true)
    })
}

/// Probes the ISP-hosted autoconfig fallback URL, keyed by the domain
/// alone (no local part).
pub fn run_isp_fallback(domain: &str) -> Option<DiscoveryAutoconfig> {
    run_probe("Autoconfig ISP fallback URL", domain, |client| {
        client.isp_fallback(domain, true)
    })
}

/// Probes the central Thunderbird ISPDB for the domain.
pub fn run_ispdb(domain: &str) -> Option<DiscoveryAutoconfig> {
    run_probe("Thunderbird ISPDB", domain, |client| {
        client.ispdb(domain, true)
    })
}

/// Runs one autoconfig probe under a spinner, returning its config on
/// success and `None` on a graceful miss the discovery chain steps
/// past. Shared by the three `run_*` entry points.
fn run_probe<F>(label: &str, domain: &str, op: F) -> Option<DiscoveryAutoconfig>
where
    F: Fn(
        &mut DiscoveryAutoconfigClientStd,
    ) -> Result<DiscoveryAutoconfig, DiscoveryAutoconfigClientStdError>,
{
    let mut client =
        DiscoveryAutoconfigClientStd::new(discovery_resolver()).with_tls(discovery_tls());

    let spinner = Spinner::start(format!("Probing {label} for {domain}…"));

    match op(&mut client) {
        Ok(config) => {
            spinner.success(summary(label, domain, &config));
            Some(config)
        }
        Err(err) => {
            trace!("{label} for {domain} failed: {err}");
            spinner.failure(format!("{label}: not available for {domain}"));
            None
        }
    }
}

/// Converts an autoconfig result into the IMAP/SMTP-only
/// [`DiscoveryResult`], keeping the first IMAP and SMTP server each.
pub fn defaults(config: &DiscoveryAutoconfig) -> DiscoveryResult {
    let imap = config
        .email_provider
        .incoming_server
        .iter()
        .find(|s| matches!(s.r#type, DiscoveryServerType::Imap))
        .and_then(imap_from_server);

    let smtp = config
        .email_provider
        .outgoing_server
        .iter()
        .find(|s| matches!(s.r#type, DiscoveryServerType::Smtp))
        .and_then(smtp_from_server);

    DiscoveryResult {
        imap,
        smtp,
        sieve: None,
    }
}

fn summary(label: &str, domain: &str, config: &DiscoveryAutoconfig) -> String {
    let has_imap = config
        .email_provider
        .incoming_server
        .iter()
        .any(|s| matches!(s.r#type, DiscoveryServerType::Imap));
    let has_smtp = config
        .email_provider
        .outgoing_server
        .iter()
        .any(|s| matches!(s.r#type, DiscoveryServerType::Smtp));

    let mut protos = Vec::with_capacity(2);
    if has_imap {
        protos.push("IMAP");
    }
    if has_smtp {
        protos.push("SMTP");
    }

    if protos.is_empty() {
        format!("{label}: configuration found for {domain} (no IMAP/SMTP fields)")
    } else {
        format!("{label}: discovered {} for {domain}", protos.join(" + "))
    }
}

fn imap_from_server(server: &DiscoveryServer) -> Option<WizardImapConfig> {
    let host = server.hostname.clone()?;
    let encryption = match server.socket_type {
        Some(DiscoverySecurityType::Tls) => ImapEncryption::Tls,
        Some(DiscoverySecurityType::Starttls) => ImapEncryption::StartTls,
        _ => ImapEncryption::None,
    };
    let port = server.port.unwrap_or(match encryption {
        ImapEncryption::Tls => 993,
        _ => 143,
    });

    Some(WizardImapConfig {
        host,
        port,
        encryption,
        login: String::new(),
        auth: ImapAuth::Password(ImapSecret::Raw(String::new().into())),
    })
}

fn smtp_from_server(server: &DiscoveryServer) -> Option<WizardSmtpConfig> {
    let host = server.hostname.clone()?;
    let encryption = match server.socket_type {
        Some(DiscoverySecurityType::Tls) => SmtpEncryption::Tls,
        Some(DiscoverySecurityType::Starttls) => SmtpEncryption::StartTls,
        _ => SmtpEncryption::None,
    };
    let port = server.port.unwrap_or(match encryption {
        SmtpEncryption::Tls => 465,
        SmtpEncryption::StartTls => 587,
        SmtpEncryption::None => 25,
    });

    Some(WizardSmtpConfig {
        host,
        port,
        encryption,
        login: String::new(),
        auth: SmtpAuth::Password(SmtpSecret::Raw(String::new().into())),
    })
}
