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
    /// ManageSieve (RFC 5804), the account's `sieve` block.
    Sieve,
}

impl Protocol {
    /// Every protocol, in the order an account serves them.
    pub const ALL: [Self; 3] = [Self::Imap, Self::Smtp, Self::Sieve];

    /// The block name in the configuration, which is also the suffix of
    /// the socket the protocol is served on.
    pub fn as_str(self) -> &'static str {
        self.cleartext_scheme()
    }

    /// The two schemes the block's `server` accepts: the cleartext one
    /// first, the implicit-TLS one second.
    pub fn schemes(self) -> [&'static str; 2] {
        match self {
            Self::Imap => ["imap", "imaps"],
            Self::Smtp => ["smtp", "smtps"],
            // NOTE: RFC 5804 registers no implicit-TLS scheme, STARTTLS
            // being the upgrade path it defines. `sieves` is this
            // project's name for the deployments listening for a
            // handshake straight away, matching himalaya.
            Self::Sieve => ["sieve", "sieves"],
        }
    }

    /// The cleartext scheme, which a STARTTLS upgrade starts from.
    pub fn cleartext_scheme(self) -> &'static str {
        self.schemes()[0]
    }

    /// The implicit-TLS scheme, which is encrypted from the first byte.
    pub fn tls_scheme(self) -> &'static str {
        self.schemes()[1]
    }

    /// The scheme a bare authority takes.
    ///
    /// IMAP and SMTP take their implicit-TLS scheme, both registering a
    /// port for it. ManageSieve registers one port and reaches TLS on it
    /// through STARTTLS, so a bare authority there is cleartext and the
    /// upgrade is what secures it.
    pub fn default_scheme(self) -> &'static str {
        match self {
            Self::Imap | Self::Smtp => self.tls_scheme(),
            Self::Sieve => self.cleartext_scheme(),
        }
    }

    /// Whether a server on `scheme` is upgraded with STARTTLS when the
    /// block does not say.
    ///
    /// Only ManageSieve defaults it on, and only on its cleartext
    /// scheme: the specification defines the upgrade as the way to
    /// reach TLS on its single port, so a bare authority that took the
    /// cleartext scheme still ends up encrypted. IMAP and SMTP have an
    /// implicit-TLS scheme of their own, so theirs stays opt-in.
    pub fn default_starttls(self, scheme: &str) -> bool {
        matches!(self, Self::Sieve) && scheme == self.cleartext_scheme()
    }

    /// The ALPN identifiers offered when the block declares none
    /// <sup>[rfc7595]</sup>.
    ///
    /// ManageSieve registers none, so its list is empty.
    ///
    /// [rfc7595]: https://www.iana.org/go/rfc7595
    pub fn default_alpn(self) -> Vec<String> {
        match self {
            Self::Imap | Self::Smtp => vec![String::from(self.as_str())],
            Self::Sieve => Vec::new(),
        }
    }

    /// The port a portless URL connects to, picked from its scheme: 143
    /// and 993 for IMAP, 25 and 465 for SMTP, 4190 for ManageSieve
    /// either way.
    ///
    /// The url crate only knows default ports for web schemes, so every
    /// scheme here needs its own answer.
    pub fn default_port(self, scheme: &str) -> u16 {
        match (self, scheme) {
            (Self::Imap, "imaps") => 993,
            (Self::Imap, _) => 143,
            (Self::Smtp, "smtps") => 465,
            (Self::Smtp, _) => 25,
            // NOTE: RFC 5804 registers 4190 alone, and `sieves` rides
            // the same port rather than claiming a second one.
            (Self::Sieve, _) => 4190,
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
    fn a_bare_authority_always_ends_up_encrypted() {
        // NOTE: either the default scheme is the implicit-TLS one, or
        // it is cleartext and STARTTLS is defaulted on. Neither leaves
        // a bare authority talking in the clear.
        for protocol in Protocol::ALL {
            let scheme = protocol.default_scheme();
            let tls = scheme == protocol.tls_scheme();

            assert!(
                tls || protocol.default_starttls(scheme),
                "{protocol} would default to cleartext",
            );
        }
    }

    #[test]
    fn only_managesieve_shares_a_port_between_its_schemes() {
        for protocol in Protocol::ALL {
            let [cleartext, tls] = protocol.schemes().map(|s| protocol.default_port(s));
            let shared = cleartext == tls;

            assert_eq!(
                shared,
                protocol == Protocol::Sieve,
                "{protocol} port sharing is wrong",
            );
        }
    }
}
