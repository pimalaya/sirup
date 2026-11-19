---
cairn: log
change: cleartext-auth-gated-on-sieve
landed: 2026-11-19
---

# The cleartext-auth key is gated on the sieve feature

`ServerConfig::allow_cleartext_auth` and `Connection::allow_cleartext_auth` carry `#[cfg(feature = "sieve")]`: only the ManageSieve session reads the switch (io-imap and io-smtp take no such option), so an IMAP-only or SMTP-only build failed clippy on a field never read. A build without `sieve` now refuses `allow-cleartext-auth` as an unknown key rather than accepting and ignoring it.

A `features` job in .github/workflows/tests.yml runs clippy with `-D warnings` over each TLS provider crossed with each protocol alone and all three, so the single-protocol sets stay green.

Capability moved: socket-proxy (the ManageSieve upstream session requirement names the gate).
