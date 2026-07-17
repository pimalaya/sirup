# Design

The settled design decisions behind Sirup, and the alternatives weighed against them.

## Socket-proxy daemon

Sirup separates the credential-holding, TLS-negotiating, authenticated half of a mail session from the clients that use it. The start command opens the upstream connection, performs the TLS handshake and SASL authentication once, binds a Unix socket, and from then on proxies raw bytes between the socket and the upstream stream. Any local tool that can read and write a Unix socket then speaks raw IMAP or SMTP without ever seeing the credentials or repeating the handshake.

This is why the binary is meant to run as a long-lived daemon, one instance per account, behind a systemd service or equivalent: the cost of connecting and authenticating is paid once, and short-lived clients attach and detach freely.

## Greeting replacement

A real client expects a greeting before its first command. Sirup synthesizes a pre-authenticated one rather than forwarding the upstream greeting, which was already consumed during connect. IMAP emits an untagged PREAUTH greeting carrying the capability list the upstream advertised after authentication; SMTP emits a 220 ready line. The two protocols keep separate framing, so a single protocol-agnostic greeting was rejected.

## Keepalive

The accept loop is non-blocking and polls with a short timeout so it can interleave two duties: accepting a new client and keeping the upstream session warm while idle. A NOOP is issued on a four-minute cadence, chosen to sit under both the IMAP thirty-minute server-side minimum and the SMTP five-minute receiver timeout, with margin for slow round-trips. Real client traffic resets the timer, so the NOOP only fires during genuine idleness.

## Single concrete stream type

The protocol clients box their stream as a trait object (Box<dyn ImapStream> or Box<dyn SmtpStream>) to stay transport-agnostic. Sirup always opens its streams through pimalaya-stream, so the concrete type is always StreamStd. To set read timeouts on the proxy loop it downcasts the trait object back to StreamStd; the downcast is infallible by construction and documented as such at the call site.

## Discovery chain

The wizard resolves an account from a single email, URL or domain input by probing sources in a fixed order and taking the first non-empty result: PACC, then Thunderbird Autoconfig (ISP main URL, ISP fallback URL, then the ISPDB), then RFC 6186 SRV records. Direct imap or smtp URLs skip discovery entirely. Any JMAP endpoint a probe surfaces is dropped at the source, since Sirup only routes the two SASL-mediated mail protocols.

## In-memory wizard

The wizard never writes to disk: it builds an AccountConfig held in memory for the current run only. This keeps throwaway sessions credential-clean and lets an operator hand off a running daemon without exposing stored secrets. The --no-account flag forces the wizard even when a configuration file exists.
