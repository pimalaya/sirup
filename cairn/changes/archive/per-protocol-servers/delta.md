---
cairn: delta
id: per-protocol-servers
---

## ADDED Requirements

- configuration: an account declares one server per protocol, in an `imap` or `smtp` block carrying that protocol's own TLS, STARTTLS, ALPN and SASL settings.
- configuration: an account declaring no block at all is named as the error it is.
- configuration: an account's secrets resolve through one memoizing resolver, so a command named by two blocks is spawned once.
- socket-proxy: `start` takes the protocols to serve as a positional list, defaulting to every block the account declares.
- socket-proxy: every upstream is opened and authenticated before any socket is bound, and an upstream that cannot open aborts the whole run.
- socket-proxy: each served protocol gets its own accept loop and its own keepalive cadence.

## MODIFIED Requirements

- configuration: the server scheme is optional, a block knowing its own protocol; a bare authority takes the implicit-TLS scheme.
- socket-proxy: the socket path keys on the protocol as well as the account, and `sock-file` overrides it per block.
- wizard: discovery returning both an IMAP and an SMTP endpoint generates both blocks rather than asking which to keep.
- wizard / `repl`: the protocol is a positional, required only when the account declares more than one.

## REMOVED Requirements

- configuration: the account-level `server`, `tls`, `starttls`, `alpn` and `sasl` fields, and the rule making the URL scheme mandatory.
