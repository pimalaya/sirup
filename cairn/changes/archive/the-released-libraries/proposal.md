---
cairn: change
id: the-released-libraries
status: landed
created: 2026-11-16
---

# Move onto the released libraries, the wizard leaving

## Why

Sirup built on pimalaya-cli 0.2 through a `[patch.crates-io]` git pin (`55b6693`), pimalaya-config 0.2, pimalaya-stream 0.3, io-imap 0.6, io-smtp 0.3, io-managesieve 0.1, io-sasl 0.1 and io-pim-discovery 0.7, none of which the rest of the family still uses. A pin cannot be published and the family moved on together, so Sirup has to take the released versions from crates.io.

The wizard cannot follow. pimalaya-cli 0.4 removed the `wizard` module it was built on (the IMAP and SMTP endpoint types and the OS keyring and token pickers), pimalaya-config 0.4 removed `toml::to_string`, its renderer, and the owner decided on 2026-11-08 that the Pimalaya CLIs drop their setup wizards. Himalaya, Comodoro, Calendula, Cardamum and Carillon already did, meeting a first run with pimalaya-cli's shared welcome. Keeping Sirup's would mean vendoring the 637-line keyring picker and porting it to io-pim-discovery 0.14 for a flow the family retired.

## What

- Delete src/wizard/, the `configure` command and its `wizard` alias, the `sirup-configure` schema, `session::test`, `AccountConfig::render`, the `Serialize` derives and `skip_serializing_if` predicates that only rendered a generated account, the `discovery` cfg of build.rs and the io-pim-discovery dependency with its TLS forwards.
- A bare `sirup` and any command finding no configuration fail with pimalaya-config's `ConfigMissingError`; `main` meets it with pimalaya-cli's `welcome::Welcome` when a person reads the output, as Carillon does, and prints the failure otherwise. A bare `sirup` over a configuration prints the help.
- Accounts resolve through `TomlConfig::load` and `take_account`, which word the three failures Sirup used to word itself, refuse several defaults, and read `-a default` as a name.
- Parse through `pimalaya_cli::clap::parse`, so a refused command line prints the failure object under `--json`.
- Port the three connects to their `*ClientStdConnectOptions`, drop the local `opt_shell_expanded_path` for pimalaya-config's, map `tls.cert` to `TlsServer`, and drop the `nom` entry imap-codec alpha.10 no longer needs.

## Scope / non-goals

`-c` keeps Sirup's own `ConfigPathsArg` (`SIRUP_CONFIG`, `:`-delimited) rather than pimalaya-cli's `ConfigFlags`. The pre-existing dead-code warnings of a build without a TLS provider or without `sieve` are left, such builds being unsupported family-wide.
