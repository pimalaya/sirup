# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added the `start` command, spawning a pre-authenticated IMAP or SMTP session and exposing it on a Unix socket.
- Added the `repl` command, a reference client that forwards raw commands to the socket-backed session.
- Added TOML configuration with per-account server address, TLS, STARTTLS, ALPN and SASL settings.
- Added an in-memory first-run wizard resolving accounts through PACC, Thunderbird Autoconfig and RFC 6186 SRV discovery.
- Added TLS support (rustls-ring, rustls-aws, native-tls) and SASL support (anonymous, login, plain, oauthbearer, xoauth2, scram-sha-256).

[unreleased]: https://github.com/pimalaya/sirup/compare/root..HEAD
