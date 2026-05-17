# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Replaced the deprecated `pimalaya-toolbox` dependency with the split crates: `pimalaya-cli` (terminal helpers + build script utilities), `pimalaya-config` (TOML loader and secret resolution) and `pimalaya-stream` (TLS / SASL types and blocking std streams).
- Switched protocol clients to the standard, blocking clients shipped by `io-imap` and `io-smtp` (`ImapClientStd::connect` / `SmtpClientStd::connect`).
- Reshaped the `[accounts.<name>.sasl]` table to a tagged enum (`sasl.anonymous` / `sasl.login` / `sasl.plain`); exactly one mechanism per account, and the whole table is now optional. The legacy `sasl.mechanisms = […]` array is removed.

[unreleased]: https://github.com/pimalaya/sirup/compare/root..HEAD
