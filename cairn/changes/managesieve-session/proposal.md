---
cairn: change
id: managesieve-session
status: active
created: 2026-08-25
---

# Serve a pre-authenticated ManageSieve session

Sirup holds IMAP and SMTP sessions open so a local tool never sees a credential. ManageSieve is the third protocol in that family and the one that benefits most: a user edits a Sieve script rarely, from a script or an editor hook, and paying a TLS handshake plus a SASL exchange for a two-command errand is the whole cost of the errand. The session also outlives the tool, which is what makes `sieve list` from a shell loop reasonable.

It fits the daemon better than either protocol already served. Sirup replaces the upstream greeting with a synthesized one, and for ManageSieve the greeting *is* the capability response, so what gets replayed is the real thing rather than the invented `220 Sirup SMTP pre-auth session ready` line SMTP has to make do with. The idle keepalive is a real `NOOP` carrying a `TAG` the reply echoes, so a keepalive reply can be told apart from anything else on the wire, which neither of the other two can promise.

Nothing is needed on the client side. io-managesieve's session coroutine skips authentication when no mechanism is given, and himalaya already passes none for a `unix://` server, so a socket Sirup binds is usable the day it exists.

## What

Add a `managesieve` cargo feature and a `Session::Managesieve` variant over `io_managesieve::client::ManagesieveClientStd`, wired the way the other two are: `connect` from the account URL, `noop` for the keepalive, the stream controls through the same downcast, and a synthesized greeting.

The greeting is the one place with a decision to make. Sirup SHALL replay the capabilities the upstream reported after authentication, minus `STARTTLS` and `SASL`: neither is reachable across the socket, the connection is already up and already authenticated, and advertising them invites a client to try. `OWNER` is worth keeping, being how a client reads back the identity the upstream settled on.

Accept `sieve://`, `sieves://` and the bare authority in the account URL, matching what himalaya accepts. Sirup's scheme is mandatory rather than defaulted, one account serving one protocol, so the bare form is out and the scheme picks ManageSieve exactly as `imap`/`smtp` pick theirs today.

## The real cost

Not the protocol. Sirup is pinned to io-imap 0.3, io-smtp 0.2, io-pim-discovery 0.3, pimalaya-cli 0.1 and pimalaya-stream 0.1, each one to three majors behind. io-managesieve needs pimalaya-stream 0.3, where `StreamStd` became `Stream`, and session.rs downcasts to `StreamStd` in two places. io-imap 0.6 also moved its session opening into `ImapSessionOpen` and reshaped `connect`, and io-smtp 0.3 did the same, so the connect path is rewritten for all three protocols rather than extended for one.

So this change is a dependency sweep with a protocol addition on top, and the sweep is what should be reviewed first. Landing the bump alone, with IMAP and SMTP behaving exactly as before, is a defensible first half.

## The wizard, and what discovery already knows

io-pim-discovery is further along here than expected. It already carries `DiscoveryService::Managesieve` and resolves one out of a PACC document, at port 4190 with implicit TLS, so the wizard has an endpoint to offer wherever a provider publishes PACC. What it does not carry is the `_sieve._tcp` SRV lookup RFC 5804 section 1.8 defines: `rfc6186::discover` covers `_imap`, `_imaps`, `_submission` and `_submissions` and stops there. Mozilla autoconfig has no ManageSieve element at all.

So the wizard gets the PACC path for free and a hand-typed endpoint otherwise, which is enough to land on. Adding the SRV lookup belongs to io-pim-discovery rather than here, and would be worth doing before this change reaches the wizard properly.

That PACC assumes implicit TLS on 4190 is worth recording on its own, since it cuts against RFC 5804, which registers that port for STARTTLS and defines no implicit-TLS twin. It is why `sieves://` stays a first-class scheme rather than a courtesy: a modern discovery draft expects it to exist.
