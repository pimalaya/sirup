# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added the `configure` command, which runs the account wizard on demand. It discovers a provider, tests the connection, then writes the generated `[accounts.<name>]` table to a configuration that does not exist yet, appends it as plain text to one that does, or prints it. Appending never re-serializes the document, so comments, ordering and hand-written formatting survive, and the generated account claims `default` only when no other account does.
- Added the `json-schema` command, describing the `--json` payload of `configure`.
- Added the `--help` footer carrying the bug tracker and the sponsoring links.

### Changed

- **BREAKING**: a bare `sirup` no longer runs the wizard. It now offers to generate a configuration when it finds none, and prints the help otherwise. Run `sirup configure` to reach the wizard by name.
- **BREAKING**: renamed `completions` and `manuals` to `completion` and `manual`, the plural staying as a hidden alias.
- Named the three configuration failures: a missing configuration file names the path it looked for, an unknown `-a` name lists the accounts the configuration does hold, and a missing default names both ways of picking one. All three used to be a bare "Cannot find account".
- Nothing prompts anymore when the standard input is not a terminal or `--json` is set, and the generated document goes to the standard output whenever the standard output is redirected.

### Fixed

- Shell-expanded `socks-dir`, `sock-file` and `tls.cert` when the configuration is read. A `~` or a `$VAR` in any of them used to be taken literally, so `socks-dir = "~/run"` bound the socket under a directory named `~`.
- Paired the upstream stream's retry strategy with the proxy loop's non-blocking mode. pimalaya-stream 0.3 retries a socket reporting it is not ready, which is what an idle proxy pass looks like, so every pass would have stalled for a minute before failing.

## [0.1.0] - 2026-07-26

### Added

- Added the `start` command: opens and authenticates an IMAP or SMTP session once, then proxies it over a per-account Unix socket so local clients speak the raw protocol without holding the credentials or repeating the handshake.
- Added the `repl` command, a reference client that forwards raw commands to the socket-backed session.
- Added the account wizard, run by bare `sirup` (no subcommand).

  Resolves an account from an email, URL or domain through PACC, Thunderbird Autoconfig and RFC 6186 SRV discovery (IMAP and SMTP only), prompts for secrets through the OS keyring, a command or a raw value, tests the account by connecting once, then prints a ready-to-save `[accounts.<name>]` fragment on stdout (`sirup >> <config>` appends it), or a JSON object with `--json`.

- Added TOML configuration with per-account server address, TLS, STARTTLS, ALPN and SASL settings.
- Added TLS support (rustls-ring, rustls-aws, native-tls) and SASL support (anonymous, login, plain, oauthbearer, xoauth2, scram-sha-256).

[unreleased]: https://github.com/pimalaya/sirup/compare/v0.1.0...master
[0.1.0]: https://github.com/pimalaya/sirup/compare/root...v0.1.0
