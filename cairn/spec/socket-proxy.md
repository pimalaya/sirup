---
cairn: spec
capability: socket-proxy
status: current
---

# Socket proxy

Sirup separates the credential-holding, TLS-negotiating, authenticated half of a mail session from the clients that use it. The `start` command opens the upstream connection, performs the TLS handshake and SASL authentication once, binds a per-account Unix socket, and from then on proxies raw bytes between the socket and the upstream stream. Any local tool that can read and write a Unix socket speaks raw IMAP or SMTP without ever seeing the credentials or repeating the handshake.

### Requirement: Authenticate once, proxy bytes
`sirup start` SHALL open the upstream connection, complete the TLS handshake and SASL authentication a single time, then bind a Unix socket and proxy raw bytes in both directions between an attached client and the upstream stream.

#### Scenario: Client attaches after authentication
- GIVEN a `sirup start` daemon that has authenticated an upstream session and bound its socket
- WHEN a local client connects to the socket and writes a protocol command
- THEN the bytes are forwarded to the upstream stream and the response is forwarded back, without any further handshake or credential exchange

### Requirement: One long-lived instance per account
Sirup SHALL run as a long-lived daemon, one instance per account, so the cost of connecting and authenticating is paid once while short-lived clients attach and detach freely.

#### Scenario: Successive clients reuse the session
- GIVEN a running daemon for one account
- WHEN two clients attach and detach in sequence
- THEN both reuse the same upstream authenticated session with no reconnect between them

### Requirement: Single concrete stream downcast
The protocol clients box their stream as a trait object to stay transport-agnostic. Sirup always opens its streams through `pimalaya-stream`, so the concrete type is always `StreamStd`. To set read timeouts on the proxy loop, Sirup SHALL downcast the boxed stream back to `StreamStd`. The downcast is infallible by construction and SHALL be documented as such at the call site.

#### Scenario: Read timeout is applied
- GIVEN a boxed upstream stream inside the proxy loop
- WHEN Sirup needs to set a read timeout
- THEN it downcasts to the concrete `StreamStd` and the downcast never fails
