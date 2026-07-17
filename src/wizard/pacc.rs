//! PACC step of the wizard's discovery chain. IMAP/SMTP only; any
//! JMAP endpoint reported by PACC is ignored since sirup speaks only
//! the SASL-mediated mail protocols.

use io_pim_discovery::pacc::{client::DiscoveryPaccClientStd, config::DiscoveryPaccConfig};
use log::trace;
use pimalaya_cli::{
    spinner::Spinner,
    wizard::{
        imap::{Encryption as ImapEncryption, ImapAuth, ImapSecret, WizardImapConfig},
        smtp::{Encryption as SmtpEncryption, SmtpAuth, SmtpSecret, WizardSmtpConfig},
    },
};

use crate::wizard::discover::{DiscoveryResult, discovery_resolver, discovery_tls};

/// Probes the domain's PACC `.well-known` endpoint under a spinner,
/// returning its config on success and `None` when the probe finds
/// nothing (a graceful miss the discovery chain steps past).
pub fn run(domain: &str) -> Option<DiscoveryPaccConfig> {
    let spinner = Spinner::start(format!("Probing PACC for {domain}…"));
    let mut client = DiscoveryPaccClientStd::new(discovery_resolver()).with_tls(discovery_tls());

    match client.discover(domain) {
        Ok(config) => {
            spinner.success(summary(domain, &config));
            Some(config)
        }
        Err(err) => {
            trace!("PACC discovery for {domain} failed: {err}");
            spinner.failure(format!("PACC: no valid configuration for {domain}"));
            None
        }
    }
}

/// Converts a PACC config into the IMAP/SMTP-only [`DiscoveryResult`],
/// assuming the implicit-TLS defaults (993 for IMAP, 465 for SMTP).
pub fn defaults(config: &DiscoveryPaccConfig) -> DiscoveryResult {
    let imap = config.protocols.imap.as_ref().map(|p| WizardImapConfig {
        host: p.host.clone(),
        port: 993,
        encryption: ImapEncryption::Tls,
        login: String::new(),
        auth: ImapAuth::Password(ImapSecret::Raw(String::new().into())),
    });

    let smtp = config.protocols.smtp.as_ref().map(|p| WizardSmtpConfig {
        host: p.host.clone(),
        port: 465,
        encryption: SmtpEncryption::Tls,
        login: String::new(),
        auth: SmtpAuth::Password(SmtpSecret::Raw(String::new().into())),
    });

    DiscoveryResult { imap, smtp }
}

fn summary(domain: &str, config: &DiscoveryPaccConfig) -> String {
    let p = &config.protocols;
    let mut protos = Vec::with_capacity(2);
    if p.imap.is_some() {
        protos.push("IMAP");
    }
    if p.smtp.is_some() {
        protos.push("SMTP");
    }
    if protos.is_empty() {
        format!("PACC: configuration found for {domain} (no IMAP/SMTP fields)")
    } else {
        format!("PACC: discovered {} for {domain}", protos.join(" + "))
    }
}
