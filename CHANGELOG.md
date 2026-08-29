# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added ManageSieve support <sup>[rfc5804]</sup>, behind a `sieve` cargo feature enabled by default. An account declares a `sieve` block beside its `imap` and `smtp` ones, and `sirup start` serves it on `<socks-dir>/sirup/<account>-sieve.sock` like the other two.

  It is the protocol that benefits most: a user edits a Sieve script rarely, from a script or an editor hook, and paying a TLS handshake plus a SASL exchange for a two-command errand is the whole cost of the errand. It also fits the daemon better than either protocol already served. On ManageSieve the greeting *is* the capability response, so what an attached client reads is the real thing rather than the invented `220` line SMTP has to make do with, and the idle keepalive is a real `NOOP` carrying a tag the reply echoes, so a keepalive reply can be told apart from anything else on the wire.

  The replayed capabilities drop `STARTTLS` and `SASL`: neither is reachable across the socket, the connection being already encrypted and already authenticated, and advertising either invites a client to attempt it. `OWNER` is kept, being how a client reads back the identity the upstream settled on.

  RFC 5804 registers one port, 4190, and reaches TLS on it through STARTTLS, so a bare `sieve.server` authority takes `sieve://` and is upgraded rather than encrypted from the first byte, unlike the IMAP and SMTP blocks. `sieves://` is accepted for the deployments the specification does not define, listening for a handshake straight away.

- Added `<block>.allow-cleartext-auth`, letting a SASL mechanism that discloses a reusable credential run over a connection still in the clear. It is off, RFC 5804 section 5 asking for the refusal, and only the ManageSieve session enforces it today.
- Added the `configure` command, which runs the account wizard on demand. It discovers a provider, tests the connection, then writes the generated `[accounts.<name>]` table to a configuration that does not exist yet, appends it as plain text to one that does, or prints it. Appending never re-serializes the document, so comments, ordering and hand-written formatting survive, and the generated account claims `default` only when no other account does.
- Added the `json-schema` command, describing the `--json` payload of `configure`.
- Added the `--help` footer carrying the bug tracker and the sponsoring links.

### Changed

- **BREAKING**: an account declares one server per protocol rather than a single one. `server`, `sock-file`, `tls`, `starttls`, `alpn` and `sasl` move from the account into an `imap`, an `smtp` or a `sieve` block, which is the shape himalaya and the other Pimalaya tools already read, so one mailbox is one account instead of two:

  ```toml
  [accounts.fastmail]
  imap.server = "imap.fastmail.com"
  imap.sasl.plain.username = "you@fastmail.com"
  imap.sasl.plain.password.command = ["pass", "show", "fastmail"]
  smtp.server = "smtp.fastmail.com"
  smtp.sasl.plain.username = "you@fastmail.com"
  smtp.sasl.plain.password.command = ["pass", "show", "fastmail"]
  ```

  `start` takes the protocols to serve as a positional list, defaulting to every one the account declares: a bare `sirup start` serves the whole account and `sirup start imap` serves the one block, which is what a per-protocol service unit wants. Every session is opened and authenticated before any socket is bound, so a provider refusing one leaves nothing half-served, and the first session to fail afterwards ends the whole run rather than leaving a supervisor reading the unit as healthy. `repl` takes one protocol, required when the account declares more than one.

- **BREAKING**: the socket path carries the protocol: `<socks-dir>/sirup/<account>.sock` becomes `<socks-dir>/sirup/<account>-<protocol>.sock`, and `sock-file` overrides it per block. A client pointing a `unix://` server at the old path needs the new one.
- **BREAKING**: the server scheme is optional now that a block knows its own protocol. `imap.server = "imap.fastmail.com"` takes `imaps://` and `smtp.server = "smtp.fastmail.com"` takes `smtps://`, a full URL still being used verbatim and a scheme the block's protocol does not speak being rejected. `starttls` became optional too, following the protocol when omitted.
- **BREAKING**: a bare `sirup` no longer runs the wizard. It now offers to generate a configuration when it finds none, and prints the help otherwise. Run `sirup configure` to reach the wizard by name.
- An account resolves its secrets through one memoizing resolver, so a credential command several blocks name is spawned once. Blocks sharing a `pass` or `gpg` entry unlock the key a single time.
- The wizard stops asking which protocol to keep when discovery returns several endpoints, and generates a block for each around one set of credentials. PACC is the only step reporting a ManageSieve endpoint: RFC 5804 section 1.8 defines a `_sieve._tcp` SRV record io-pim-discovery does not look up yet, and Mozilla autoconfig has no ManageSieve element at all.
- **BREAKING**: renamed `completions` and `manuals` to `completion` and `manual`, the plural staying as a hidden alias.
- Named the three configuration failures: a missing configuration file names the path it looked for, an unknown `-a` name lists the accounts the configuration does hold, and a missing default names both ways of picking one. All three used to be a bare "Cannot find account".
- Nothing prompts anymore when the standard input is not a terminal or `--json` is set, and the generated document goes to the standard output whenever the standard output is redirected.

### Fixed

- Shell-expanded `socks-dir`, `sock-file` and `tls.cert` when the configuration is read. A `~` or a `$VAR` in any of them used to be taken literally, so `socks-dir = "~/run"` bound the socket under a directory named `~`.
- Stopped the IMAP `repl` spinning on `BAD Null command` when its standard input reaches end of file. It now exits, as the SMTP one already did.
- Paired the upstream stream's retry strategy with the proxy loop's non-blocking mode. pimalaya-stream 0.3 retries a socket reporting it is not ready, which is what an idle proxy pass looks like, so every pass would have stalled for a minute before failing.

## [0.1.0] - 2026-07-26

### Added

- Added the `start` command: opens and authenticates an IMAP or SMTP session once, then proxies it over a per-account Unix socket so local clients speak the raw protocol without holding the credentials or repeating the handshake.
- Added the `repl` command, a reference client that forwards raw commands to the socket-backed session.
- Added the account wizard, run by bare `sirup` (no subcommand).

  Resolves an account from an email, URL or domain through PACC, Thunderbird Autoconfig and RFC 6186 SRV discovery (IMAP and SMTP only), prompts for secrets through the OS keyring, a command or a raw value, tests the account by connecting once, then prints a ready-to-save `[accounts.<name>]` fragment on stdout (`sirup >> <config>` appends it), or a JSON object with `--json`.

- Added TOML configuration with per-account server address, TLS, STARTTLS, ALPN and SASL settings.
- Added TLS support (rustls-ring, rustls-aws, native-tls) and SASL support (anonymous, login, plain, oauthbearer, xoauth2, scram-sha-256).

[rfc5804]: https://www.rfc-editor.org/rfc/rfc5804

[unreleased]: https://github.com/pimalaya/sirup/compare/v0.1.0...master
[0.1.0]: https://github.com/pimalaya/sirup/compare/root...v0.1.0
