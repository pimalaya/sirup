# Contributing guide

Thank you for investing your time in contributing to Sirup.

Whether you are a human or an AI agent, read these in order before touching the code:

1. the [Pimalaya README](https://github.com/pimalaya) for what the project is and how its repositories stack;
2. the [Pimalaya CONTRIBUTING](https://github.com/pimalaya/.github/blob/master/CONTRIBUTING.md) guide, which chains to the shared architecture and guidelines;
3. the inline header documentation, starting with src/main.rs: it is the architecture document of this crate;
4. the docs/ folder for the development history and living plans.

Everything below documents only what differs from the Pimalaya standards.

## Where changes belong

Sirup is a thin CLI binary: it drives protocol clients and a discovery engine, then proxies an authenticated session over a Unix socket. Triage before patching, since most fixes belong upstream:

- IMAP wire semantics (greeting, capabilities, NOOP, authentication) belong in [io-imap](https://github.com/pimalaya/io-imap);
- SMTP wire semantics (greeting, EHLO, NOOP, authentication) belong in [io-smtp](https://github.com/pimalaya/io-smtp);
- service discovery consumed by the wizard (PACC, Autoconfig, RFC 6186 SRV) belongs in [io-pim-discovery](https://github.com/pimalaya/io-pim-discovery);
- the socket proxy, keepalive, configuration shape, wizard prompts and command UX live here.

The shared clap, printer, prompt and spinner primitives come from [pimalaya/cli](https://github.com/pimalaya/cli), the TOML loader and secret resolution from [pimalaya/config](https://github.com/pimalaya/config), and the TCP, TLS and SASL plumbing from [pimalaya/stream](https://github.com/pimalaya/stream).

## Feature matrix

Two axes are gated by cargo features: the protocols (imap, smtp, and scram for SCRAM-SHA-256 authentication) and the TLS provider (rustls-ring default, rustls-aws, native-tls), plus vendored. The wizard is compiled only when both protocols and a TLS provider are enabled. Build against the reduced sets to check the feature gates still hold:

```sh
cargo build --no-default-features --features imap,rustls-ring
cargo build --no-default-features --features smtp,rustls-ring
cargo build --no-default-features --features imap,smtp,native-tls
```
