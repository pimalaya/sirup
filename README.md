# Sirup [![Matrix](https://img.shields.io/badge/chat-%23pimalaya-blue?style=flat&logo=matrix&logoColor=white)](https://matrix.to/#/#pimalaya:matrix.org) [![Mastodon](https://img.shields.io/badge/news-%40pimalaya-blue?style=flat&logo=mastodon&logoColor=white)](https://fosstodon.org/@pimalaya)

CLI to spawn pre-authenticated IMAP/SMTP sessions and expose them via Unix sockets

## Table of contents

- [Features](#features)
- [Coverage](#coverage)
- [Installation](#installation)
  - [Pre-built binary](#pre-built-binary)
  - [Cargo](#cargo)
  - [Nix](#nix)
  - [Sources](#sources)
- [Configuration](#configuration)
- [Usage](#usage)
- [AI disclosure](#ai-disclosure)
- [License](#license)
- [Social](#social)
- [Contributing](#contributing)
- [Sponsoring](#sponsoring)

## Features

- **Pre-authenticated sessions**: connect and log in once, then expose the live IMAP or SMTP session on a Unix socket so any local client can speak the raw protocol without holding your credentials.
- **Account discovery wizard**: run with no subcommand and it finds a provider's servers from an email address, a server URL or a bare domain, then prints a ready-to-save account config fragment on stdout.
- **SASL authentication**: anonymous, login, plain, oauthbearer, xoauth2 and scram-sha-256 (the last requires the `scram` feature).
- **STARTTLS and implicit TLS**: pick either from the account's server scheme, with the ALPN token inferred per protocol.
- **REPL**: a built-in reference client that forwards raw commands to the socket, for testing and as an implementation example.
- **Machine-readable output**: `--json` switches supported commands to JSON for scripts.
- Full standard, blocking client with **TLS** support:
  - [Rustls](https://crates.io/crates/rustls) with ring crypto (requires `rustls-ring` feature, enabled by default)
  - [Rustls](https://crates.io/crates/rustls) with aws crypto (requires `rustls-aws` feature)
  - [Native TLS](https://crates.io/crates/native-tls) (requires `native-tls` feature)

> [!TIP]
> Sirup uses [cargo features](https://doc.rust-lang.org/cargo/reference/features.html) to gate protocol and TLS support; the default set is declared in Cargo.toml.

## Coverage

| RFC    | What is covered |
|--------|-----------------|
| [9051] | IMAP: the pre-authenticated session, capability advertisement and NOOP keepalive |
| [5321] | SMTP: the message submission session, EHLO greeting and NOOP keepalive |
| [4616] | The PLAIN SASL mechanism |
| [7628] | The OAUTHBEARER SASL mechanism |
| [7677] | The SCRAM-SHA-256 SASL mechanism (requires the `scram` feature) |
| [6186] | Locating IMAP and submission services through DNS SRV records |

[9051]: https://www.rfc-editor.org/rfc/rfc9051
[5321]: https://www.rfc-editor.org/rfc/rfc5321
[4616]: https://www.rfc-editor.org/rfc/rfc4616
[7628]: https://www.rfc-editor.org/rfc/rfc7628
[7677]: https://www.rfc-editor.org/rfc/rfc7677
[6186]: https://www.rfc-editor.org/rfc/rfc6186

Discovery also covers the [PACC](https://datatracker.ietf.org/doc/html/draft-ietf-mailmaint-pacc) draft and [Mozilla Thunderbird Autoconfiguration](https://wiki.mozilla.org/Thunderbird:Autoconfiguration), neither of which is an RFC.

## Installation

### Pre-built binary

Sirup has no stable release yet. Pre-built binaries from the master branch are available through the [releases](https://github.com/pimalaya/sirup/actions/workflows/releases.yml) GitHub workflow: open the latest run and look for the *Artifacts* section.

> [!NOTE]
> Such binaries are built with the default cargo features. If you need specific features, please use another installation method.

### Cargo

```sh
cargo install --locked --git https://github.com/pimalaya/sirup.git
```

With only IMAP support:

```sh
cargo install --locked --git https://github.com/pimalaya/sirup.git \
  --no-default-features \
  --features imap,rustls-ring
```

### Nix

If you have the [Flakes](https://wiki.nixos.org/wiki/Flakes) feature enabled:

```sh
nix profile install github:pimalaya/sirup
```

Or run without installing:

```sh
nix run github:pimalaya/sirup
```

### Sources

```sh
git clone https://github.com/pimalaya/sirup
cd sirup
nix run
```

## Configuration

Run sirup with no subcommand to launch the wizard: it asks for an email address, a server URL or a bare domain, runs provider discovery, prompts for the SASL credentials, then prints the resulting account as a ready-to-save `[accounts.<name>]` TOML fragment on stdout. Prompts render on stderr, so you can append it straight to your config:

```sh
sirup >> ~/.config/sirup/config.toml
```

Sirup never writes your configuration itself; the config stays entirely user-owned.

A configuration is loaded from the first valid path among:

- $XDG_CONFIG_HOME/sirup/config.toml
- $HOME/.config/sirup/config.toml
- $HOME/.siruprc

Override the path with -c <PATH> or SIRUP_CONFIG=<PATH>; multiple paths can be passed at once, separated by :. The first one is the base and the rest are deep-merged on top. The full field reference lives in [config.sample.toml](./config.sample.toml).

## Usage

Every command and subcommand is documented through --help. The common flows:

```sh
sirup start                    # start a pre-authenticated session for the default account
sirup start --account work     # ... for a named account
sirup repl                     # attach the reference client to the running session
```

The start command runs as a blocking daemon, best placed in a systemd service or equivalent: it connects to the server, performs the TLS negotiation and authentication, then exposes the session on a Unix socket. Any client that can read from and write to that socket can then drive the session; the greeting is replaced by an IMAP PREAUTH line (carrying the upstream capabilities) or an SMTP 220 ready line.

Logs go to stderr; --log-level and --log-file control verbosity and destination, and --json switches supported output to machine-readable objects.

## AI disclosure

This project is developed with AI assistance. This section documents how, so users and downstream packagers can make informed decisions.

- **Tools**: Claude Code (Anthropic), invoked locally with a persistent project-scoped memory and a small set of repo-specific rules.
- **Used for**: Refactors, mechanical multi-file edits, boilerplate (feature gates, error enums, derive macros, trait impls), test scaffolding, doc polish, exploratory design conversations.
- **Not used for**: Engineering, critical code, git manipulation (commit, merge, rebase…), real-world tests.
- **Verification**: Every AI-assisted change is read, compiled, tested, and formatted before commit. Behavioural correctness is verified against the relevant RFC or upstream spec, not assumed from the model output. Tests are never adjusted to fit AI-generated code; the code is adjusted to fit correct behaviour.
- **Limitations**: AI models occasionally produce code that compiles and passes tests but is subtly wrong. The verification workflow catches most of this; it does not catch all of it. Bug reports are welcome and taken seriously.
- **Last reviewed**: 17/07/2026

## License

This project is licensed under either of:

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

## Social

- Chat on [Matrix](https://matrix.to/#/#pimalaya:matrix.org)
- News on [Mastodon](https://fosstodon.org/@pimalaya) or [RSS](https://fosstodon.org/@pimalaya.rss)
- Mail at [pimalaya.org@posteo.net](mailto:pimalaya.org@posteo.net)

## Contributing

Contributions are welcome: start with [CONTRIBUTING.md](./CONTRIBUTING.md), which opens with the Pimalaya-wide guides to read first.

## Sponsoring

[![nlnet](https://nlnet.nl/logo/banner-160x60.png)](https://nlnet.nl/)

Special thanks to the [NLnet foundation](https://nlnet.nl/) and the [European Commission](https://www.ngi.eu/) that have been financially supporting the project for years:

- 2022 → 2023: [NGI Assure](https://nlnet.nl/project/Himalaya/)
- 2023 → 2024: [NGI Zero Entrust](https://nlnet.nl/project/Pimalaya/)
- 2024 → 2026: [NGI Zero Core](https://nlnet.nl/project/Pimalaya-PIM/)
- *2027 in preparation…*

If you appreciate the project, feel free to donate using one of the following providers:

[![GitHub](https://img.shields.io/badge/-GitHub%20Sponsors-fafbfc?logo=GitHub%20Sponsors)](https://github.com/sponsors/soywod)
[![Ko-fi](https://img.shields.io/badge/-Ko--fi-ff5e5a?logo=Ko-fi&logoColor=ffffff)](https://ko-fi.com/soywod)
[![Buy Me a Coffee](https://img.shields.io/badge/-Buy%20Me%20a%20Coffee-ffdd00?logo=Buy%20Me%20A%20Coffee&logoColor=000000)](https://www.buymeacoffee.com/soywod)
[![Liberapay](https://img.shields.io/badge/-Liberapay-f6c915?logo=Liberapay&logoColor=222222)](https://liberapay.com/soywod)
[![thanks.dev](https://img.shields.io/badge/-thanks.dev-000000?logo=data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMjQuMDk3IiBoZWlnaHQ9IjE3LjU5NyIgY2xhc3M9InctMzYgbWwtMiBsZzpteC0wIHByaW50Om14LTAgcHJpbnQ6aW52ZXJ0IiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjxwYXRoIGQ9Ik05Ljc4MyAxNy41OTdINy4zOThjLTEuMTY4IDAtMi4wOTItLjI5Ny0yLjc3My0uODktLjY4LS41OTMtMS4wMi0xLjQ2Mi0xLjAyLTIuNjA2di0xLjM0NmMwLTEuMDE4LS4yMjctMS43NS0uNjc4LTIuMTk1LS40NTItLjQ0Ni0xLjIzMi0uNjY5LTIuMzQtLjY2OUgwVjcuNzA1aC41ODdjMS4xMDggMCAxLjg4OC0uMjIyIDIuMzQtLjY2OC40NTEtLjQ0Ni42NzctMS4xNzcuNjc3LTIuMTk1VjMuNDk2YzAtMS4xNDQuMzQtMi4wMTMgMS4wMjEtMi42MDZDNS4zMDUuMjk3IDYuMjMgMCA3LjM5OCAwaDIuMzg1djEuOTg3aC0uOTg1Yy0uMzYxIDAtLjY4OC4wMjctLjk4LjA4MmExLjcxOSAxLjcxOSAwIDAgMC0uNzM2LjMwN2MtLjIwNS4xNTYtLjM1OC4zODQtLjQ2LjY4Mi0uMTAzLjI5OC0uMTU0LjY4Mi0uMTU0IDEuMTUxVjUuMjNjMCAuODY3LS4yNDkgMS41ODYtLjc0NSAyLjE1NS0uNDk3LjU2OS0xLjE1OCAxLjAwNC0xLjk4MyAxLjMwNXYuMjE3Yy44MjUuMyAxLjQ4Ni43MzYgMS45ODMgMS4zMDUuNDk2LjU3Ljc0NSAxLjI4Ny43NDUgMi4xNTR2MS4wMjFjMCAuNDcuMDUxLjg1NC4xNTMgMS4xNTIuMTAzLjI5OC4yNTYuNTI1LjQ2MS42ODIuMTkzLjE1Ny40MzcuMjYuNzMyLjMxMi4yOTUuMDUuNjIzLjA3Ni45ODQuMDc2aC45ODVabTE0LjMxNC03LjcwNmgtLjU4OGMtMS4xMDggMC0xLjg4OC4yMjMtMi4zNC42NjktLjQ1LjQ0NS0uNjc3IDEuMTc3LS42NzcgMi4xOTVWMTQuMWMwIDEuMTQ0LS4zNCAyLjAxMy0xLjAyIDIuNjA2LS42OC41OTMtMS42MDUuODktMi43NzQuODloLTIuMzg0di0xLjk4OGguOTg0Yy4zNjIgMCAuNjg4LS4wMjcuOTgtLjA4LjI5Mi0uMDU1LjUzOC0uMTU3LjczNy0uMzA4LjIwNC0uMTU3LjM1OC0uMzg0LjQ2LS42ODIuMTAzLS4yOTguMTU0LS42ODIuMTU0LTEuMTUydi0xLjAyYzAtLjg2OC4yNDgtMS41ODYuNzQ1LTIuMTU1LjQ5Ny0uNTcgMS4xNTgtMS4wMDQgMS45ODMtMS4zMDV2LS4yMTdjLS44MjUtLjMwMS0xLjQ4Ni0uNzM2LTEuOTgzLTEuMzA1LS40OTctLjU3LS43NDUtMS4yODgtLjc0NS0yLjE1NXYtMS4wMmMwLS40Ny0uMDUxLS44NTQtLjE1NC0xLjE1Mi0uMTAyLS4yOTgtLjI1Ni0uNTI2LS40Ni0uNjgyYTEuNzE5IDEuNzE5IDAgMCAwLS43MzctLjMwNyA1LjM5NSA1LjM5NSAwIDAgMC0uOTgtLjA4MmgtLjk4NFYwaDIuMzg0YzEuMTY5IDAgMi4wOTMuMjk3IDIuNzc0Ljg5LjY4LjU5MyAxLjAyIDEuNDYyIDEuMDIgMi42MDZ2MS4zNDZjMCAxLjAxOC4yMjYgMS43NS42NzggMi4xOTUuNDUxLjQ0NiAxLjIzMS42NjggMi4zNC42NjhoLjU4N3oiIGZpbGw9IiNmZmYiLz48L3N2Zz4=)](https://thanks.dev/soywod)
[![PayPal](https://img.shields.io/badge/-PayPal-0079c1?logo=PayPal&logoColor=ffffff)](https://www.paypal.com/paypalme/soywod)
