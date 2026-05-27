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

//! PACC step of the wizard's discovery chain. IMAP/SMTP only; any
//! JMAP endpoint reported by PACC is ignored since sirup speaks only
//! the SASL-mediated mail protocols.

use io_discovery::pacc::{client::DiscoveryPaccClientStd, types::PaccConfig};
use log::debug;
use pimalaya_cli::{
    spinner::Spinner,
    wizard::{
        imap::{Encryption as ImapEncryption, ImapAuth, ImapSecret, WizardImapConfig},
        smtp::{Encryption as SmtpEncryption, SmtpAuth, SmtpSecret, WizardSmtpConfig},
    },
};

use crate::wizard::discover::{DiscoveryResult, discovery_resolver, discovery_tls};

pub fn run(domain: &str) -> Option<PaccConfig> {
    let spinner = Spinner::start(format!("Probing PACC for {domain}…"));
    let mut client = DiscoveryPaccClientStd::new(discovery_resolver()).with_tls(discovery_tls());

    match client.discover(domain) {
        Ok(config) => {
            spinner.success(summary(domain, &config));
            Some(config)
        }
        Err(err) => {
            debug!("PACC discovery for {domain} failed: {err}");
            spinner.failure(format!("PACC: no valid configuration for {domain}"));
            None
        }
    }
}

pub fn defaults(config: &PaccConfig) -> DiscoveryResult {
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

fn summary(domain: &str, config: &PaccConfig) -> String {
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
