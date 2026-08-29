---
cairn: log
id: managesieve-session
date: 2026-08-29
---

# Serve a pre-authenticated ManageSieve session

Sirup speaks a third protocol. An account declares a `sieve` block beside its `imap` and `smtp` ones, and `sirup start` serves it on its own socket like the other two, behind a `sieve` cargo feature on by default.

ManageSieve is the protocol that gains most from the daemon. A user edits a Sieve script rarely, from a script or an editor hook, and paying a TLS handshake plus a SASL exchange for a two-command errand is the whole cost of the errand. It also fits sirup better than either protocol already served. The greeting *is* the capability response here, so what an attached client reads is the real thing rather than the invented `220` line SMTP has to make do with, and the idle keepalive is a real `NOOP` carrying a tag the reply echoes, so a keepalive reply can be told apart from anything else on the wire. An echo that does not match ends the run rather than leaving a desynchronised session for the next client.

The replayed capabilities drop `STARTTLS` and `SASL`. Neither is reachable across the socket, the connection being already encrypted and already authenticated, and advertising either invites a client to attempt it. `OWNER` stays: it is how a client reads back the identity the upstream settled on. io-managesieve's quoting helpers are crate-private, so the greeting is rendered here, escaping the backslash and the double quote and dropping a stray CR or LF, which is what keeps a capability value from forging a line of its own.

RFC 5804 is the reason `Protocol` grew more than a variant. It registers one port, 4190, and reaches TLS on it through STARTTLS, where IMAP and SMTP each register an implicit-TLS port of their own. So the scheme table split into a cleartext and a TLS half, the scheme a bare authority takes stopped being the TLS one for every protocol, and `starttls` became an `Option<bool>` following the protocol when omitted: a bare `sieve.server` authority is cleartext and upgraded, which is how it still ends up encrypted. A test pins that every protocol's bare authority ends up encrypted one way or the other.

`allow-cleartext-auth` came with it. io-managesieve refuses to send a password over a connection still in the clear, which RFC 5804 section 5 asks for, and himalaya already exposes the override; sirup exposes it on the shared block rather than inventing a ManageSieve-only field, documented as the ManageSieve session being the only one enforcing it today.

The wizard generates a `sieve` block when PACC reports one, which is the only step that can: RFC 5804 section 1.8 defines a `_sieve._tcp` SRV record io-pim-discovery does not look up yet, and Mozilla autoconfig has no ManageSieve element at all. PACC names a host and leaves the rest, so 4190 with implicit TLS is this crate's assumption rather than the document's, and it cuts against the specification, which is exactly why `sieves://` is a first-class scheme here. A build without the `sieve` feature generates no block promising a session it cannot serve.

The `repl` gained a ManageSieve half that drives io-managesieve's own client over the socket rather than reading replies by eye. A data line may carry a length-prefixed literal whose payload is free to contain anything, a line reading `OK` included, so the heuristic the SMTP client uses would truncate a downloaded script; the parser is public, so the reference client uses it.

## Verification

Not against Fastmail: it answers on no ManageSieve port, so the registered account cannot exercise this. Verified instead against a scripted server on 4190, which is enough for everything sirup owns. The session opened, the socket bound, and `sirup repl sieve` read back a greeting carrying IMPLEMENTATION, SIEVE, OWNER and VERSION with `STARTTLS` and `SASL` gone, then `LISTSCRIPTS` and `NOOP` proxied through with their responses framed by the library. The idle keepalive was watched over a five-minute run. Three unit tests pin the greeting: what it replays, what it drops, and that a capability value carrying a quote and a CRLF cannot forge a second line.

Capabilities moved: socket-proxy, greeting, keepalive, configuration, wizard.
