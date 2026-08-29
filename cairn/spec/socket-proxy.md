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

### Requirement: Global account flag
`-a` / `--account` SHALL be a global flag on the root parser, selecting the account for whichever subcommand runs.

#### Scenario: Named account
- GIVEN a config with several accounts
- WHEN `sirup -a work start` runs
- THEN the `work` account is used

### Requirement: Commands resolve the account from config
`start` and `repl` SHALL resolve their account from the loaded config by the global account name, or the `default = true` account when none is given. Each of the three ways that fails is a hard error naming what is missing, described under the configuration capability, and a missing configuration raises the wizard offer before it errors.

#### Scenario: Missing config
- GIVEN no config file on disk
- WHEN `sirup start` runs
- THEN the wizard is offered, and the command still fails naming the path it looked for when no configuration follows

### Requirement: Single concrete stream downcast
The protocol clients box their stream as a trait object to stay transport-agnostic. Sirup always opens its streams through `pimalaya-stream`, so the concrete type is always its `Stream`. To reach the stream controls the proxy loop drives, Sirup SHALL downcast the boxed stream back to `Stream`. The downcast is infallible by construction and SHALL be documented as such at the call site.

#### Scenario: Read timeout is applied
- GIVEN a boxed upstream stream inside the proxy loop
- WHEN Sirup needs to set a read timeout
- THEN it downcasts to the concrete `Stream` and the downcast never fails

### Requirement: Non-blocking mode takes the retry strategy with it
A `pimalaya-stream` stream retries a read or a write that reports it is not ready, for a minute by default, which is exactly what an idle proxy pass looks like. Sirup SHALL set the retry strategy alongside the non-blocking mode: taken off when the proxy drives the stream non-blocking, restored when it goes back to blocking for the keepalive NOOP.

#### Scenario: Idle proxy pass
- GIVEN the proxy loop has put the upstream in non-blocking mode
- WHEN a pass finds nothing to relay
- THEN the read hands back a not-ready failure at once, rather than being retried for a minute and then failing as a timeout
