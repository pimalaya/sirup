---
cairn: delta
change: managesieve-session
---

## ADDED Requirements

### Requirement: ManageSieve upstream session
Sirup SHALL serve a third protocol, ManageSieve, behind a `managesieve` cargo feature. The account URL scheme SHALL select it: `sieve` for STARTTLS or cleartext and `sieves` for implicit TLS, alongside the existing `imap`, `imaps`, `smtp` and `smtps`. The upstream session SHALL be opened, TLS-negotiated and authenticated once by io-managesieve, exactly as the other two protocols are by io-imap and io-smtp.

#### Scenario: ManageSieve client attaches
- GIVEN a `sirup start` daemon that has authenticated an upstream ManageSieve session and bound its socket
- WHEN a local client connects to the socket and writes `LISTSCRIPTS`
- THEN the bytes are forwarded upstream and the response is forwarded back, without any further handshake or credential exchange

## MODIFIED Requirements

### Requirement: Synthesize a pre-authenticated greeting
On IMAP, Sirup SHALL emit an untagged `PREAUTH` greeting carrying the capability list the upstream advertised after authentication. On SMTP, Sirup SHALL emit a `220` ready line. On ManageSieve, Sirup SHALL emit the capability response the upstream reported after authentication, followed by an `OK` completion. Sirup SHALL NOT forward the upstream greeting, which was already consumed during connect.

The replayed ManageSieve capabilities SHALL omit `STARTTLS` and `SASL`. Neither is reachable across the socket, the connection being already encrypted and already authenticated, and advertising either invites an attached client to attempt it.

#### Scenario: IMAP client attaches
- GIVEN an authenticated upstream IMAP session
- WHEN a client attaches to the socket
- THEN Sirup emits an untagged `PREAUTH` greeting including the post-authentication capabilities

#### Scenario: SMTP client attaches
- GIVEN an authenticated upstream SMTP session
- WHEN a client attaches to the socket
- THEN Sirup emits a `220` ready line

#### Scenario: ManageSieve client attaches
- GIVEN an authenticated upstream ManageSieve session
- WHEN a client attaches to the socket
- THEN Sirup emits the post-authentication capability lines followed by `OK`, and neither `STARTTLS` nor `SASL` appears among them

### Requirement: Idle NOOP cadence
While idle, Sirup SHALL issue a NOOP on a four-minute cadence, chosen to sit under both the IMAP thirty-minute server-side minimum and the SMTP five-minute receiver timeout, with margin for slow round-trips. The ManageSieve inactivity timeout is thirty minutes at the least once authenticated, so the same cadence covers it. Real client traffic SHALL reset the timer, so the NOOP fires only during genuine idleness.

The ManageSieve NOOP SHALL carry a tag, which the server echoes back in the `TAG` response code, so a keepalive reply is distinguishable from anything else arriving on the stream.

#### Scenario: Session kept warm while idle
- GIVEN a running daemon with no attached client
- WHEN four minutes elapse without traffic
- THEN Sirup issues a NOOP to the upstream session and the session stays authenticated

#### Scenario: Client traffic defers the NOOP
- GIVEN a running daemon
- WHEN a client sends traffic before the cadence elapses
- THEN the keepalive timer is reset and no idle NOOP is issued
