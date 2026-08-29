//! # Protocol
//!
//! The protocol an account declares a server for, and everything that
//! follows from it: the schemes its URL accepts, the one a bare authority
//! takes, the ALPN identifier its TLS handshake offers and the port it
//! listens on.
//!
//! It is the key of an account's server table and the value `start` and
//! `repl` take on the command line, so one type serves the schema and the
//! parser both.
//!
//! The defaults mirror io-imap's and io-smtp's own `default_alpn()` and
//! `default_port()`, kept local so the schema depends on no backend crate
//! and resolves the same under any feature subset.

use std::fmt;

use clap::ValueEnum;

/// A protocol Sirup opens an upstream session for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Protocol {
    // NOTE: these render in `--help` as the value list of the `start`
    // and `repl` positionals, so they carry no rustdoc markup: clap
    // prints a doc comment verbatim.
    /// IMAP (RFC 9051), the account's `imap` block.
    Imap,
    /// SMTP submission (RFC 6409), the account's `smtp` block.
    Smtp,
}

impl Protocol {
    /// Every protocol, in the order an account serves them.
    pub const ALL: [Self; 2] = [Self::Imap, Self::Smtp];

    /// The block name in the configuration, which is also the suffix of
    /// the socket the protocol is served on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Imap => "imap",
            Self::Smtp => "smtp",
        }
    }

    /// The scheme a bare authority takes, implicit TLS in both cases.
    pub fn default_scheme(self) -> &'static str {
        match self {
            Self::Imap => "imaps",
            Self::Smtp => "smtps",
        }
    }

    /// The schemes the block's `server` accepts, cleartext first.
    pub fn schemes(self) -> &'static [&'static str] {
        match self {
            Self::Imap => &["imap", "imaps"],
            Self::Smtp => &["smtp", "smtps"],
        }
    }

    /// The ALPN identifiers offered when the block declares none
    /// <sup>[rfc7595]</sup>.
    ///
    /// [rfc7595]: https://www.iana.org/go/rfc7595
    pub fn default_alpn(self) -> Vec<String> {
        vec![String::from(self.as_str())]
    }

    /// The port a portless URL connects to, picked from its scheme: 143
    /// and 993 for IMAP, 25 and 465 for SMTP.
    ///
    /// The url crate only knows default ports for web schemes, so every
    /// scheme here needs its own answer.
    pub fn default_port(self, scheme: &str) -> u16 {
        match (self, scheme) {
            (Self::Imap, "imaps") => 993,
            (Self::Imap, _) => 143,
            (Self::Smtp, "smtps") => 465,
            (Self::Smtp, _) => 25,
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_scheme_is_one_of_the_accepted_ones() {
        for protocol in Protocol::ALL {
            assert!(protocol.schemes().contains(&protocol.default_scheme()));
        }
    }

    #[test]
    fn every_accepted_scheme_has_a_port() {
        // NOTE: the cleartext and implicit-TLS ports differ, which is
        // what a portless URL relies on to reach the right one.
        for protocol in Protocol::ALL {
            let ports: Vec<u16> = protocol
                .schemes()
                .iter()
                .map(|scheme| protocol.default_port(scheme))
                .collect();

            assert_eq!(ports.len(), 2);
            assert_ne!(ports[0], ports[1]);
        }
    }
}
