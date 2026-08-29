---
cairn: change
id: lib-bump
status: landed
created: 2026-08-29
---

# Move onto the current libraries

## Why

The libraries sirup builds on all moved at once, and sirup no longer compiles against them.

- pimalaya-stream 0.3 dropped the `std::stream::StreamStd` type for a flat `stream::Stream`, handed the SASL vocabulary over to io-sasl, and gave every stream a retry strategy.
- io-imap 0.6 and io-smtp 0.3 collapsed their per-argument connect signatures into a session options struct, take io-sasl credentials, moved `noop` behind the `ImapClient` and `SmtpClient` traits, and both now return the negotiated capabilities beside the client.
- io-sasl 0.1 is the new home of the mechanisms and their credential structs, which the two protocol crates used to carry.
- pimalaya-config 0.2 made `Secret::Command` hold a `CommandConfig` rather than a built `std::process::Command`.

The retry strategy is the one that changes behaviour rather than spelling. A stream now retries a read or a write that reports it is not ready, for a minute by default. The proxy loop is built on the opposite promise: it drives the upstream non-blocking and treats `WouldBlock` as "nothing to relay this pass". Left alone, every idle pass would spend a minute inside the retry loop before failing with a timeout, which is the proxy hanging.

## What

Migrate the four call sites, and take the retry strategy off the stream whenever the proxy puts it in non-blocking mode, restoring it when it goes back to blocking for the keepalive NOOP.
