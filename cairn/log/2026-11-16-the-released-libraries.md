---
cairn: log
id: the-released-libraries
date: 2026-11-16
---

# Moved onto the released libraries, the wizard left

Sirup takes pimalaya-cli 0.6, pimalaya-config 0.5, pimalaya-stream 0.4, io-imap 0.11, io-smtp 0.9, io-managesieve 0.4 and io-sasl 0.2 from crates.io, with no `[patch.crates-io]` left: the git pin of pimalaya-cli `55b6693` is gone, and so is the `nom` entry, imap-codec 2.0.0-alpha.10 building without it.

The wizard left with the pimalaya-cli `wizard` module it was built on, following the owner's 2026-11-08 decision: src/wizard/, the `configure` command and its `wizard` alias, the `sirup-configure` schema, `session::test`, `AccountConfig::render`, the `Serialize` derives, the `discovery` cfg of build.rs and the io-pim-discovery dependency. A missing configuration now fails with pimalaya-config's `ConfigMissingError`, which `main` meets with pimalaya-cli's `welcome::Welcome` when a person reads the output, Carillon's pattern; `cli::welcome` is tested for its download and copy commands. Accounts resolve through `TomlConfig::load` and `take_account`, which refuse several defaults and read `-a default` as a name. The three connects take their `*ClientStdConnectOptions`, `tls.cert` maps to `TlsServer`, and the local `opt_shell_expanded_path` gave way to pimalaya-config's.

Verified: `cargo fmt --all --check`, `cargo clippy --all-features --all-targets -- -D warnings` and `cargo test --all-features` (14 tests) pass, `cargo-deny check` is clean, and the release builds `nix build .#cross-x86_64-linux` and `.#cross-x86_64-windows` succeed. The `imap`, `smtp` and `sieve` builds each on one TLS provider compile; a build without TLS or without `sieve` keeps its earlier dead-code warnings.

Capabilities moved: configuration (A person missing a configuration meets the welcome and Several defaults are refused added; A missing configuration names the path and An unknown account lists the ones that exist modified; A missing configuration raises the offer first removed), socket-proxy (Commands resolve the account from config modified), wizard and discovery removed whole.
