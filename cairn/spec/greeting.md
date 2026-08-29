---
cairn: spec
capability: greeting
status: current
---

# Greeting replacement

A real client expects a greeting before its first command. The upstream greeting was already consumed during connect, so Sirup synthesizes a pre-authenticated one for each attaching client instead of forwarding it. The two protocols keep separate framing, so the greeting is protocol-specific.

### Requirement: Synthesize a pre-authenticated greeting
On IMAP, Sirup SHALL emit an untagged `PREAUTH` greeting carrying the capability list the upstream advertised after authentication. On SMTP, Sirup SHALL emit a `220` ready line. Sirup SHALL NOT forward the upstream greeting, which was already consumed during connect.

#### Scenario: IMAP client attaches
- GIVEN an authenticated upstream IMAP session
- WHEN a client attaches to the socket
- THEN Sirup emits an untagged `PREAUTH` greeting including the post-authentication capabilities

#### Scenario: SMTP client attaches
- GIVEN an authenticated upstream SMTP session
- WHEN a client attaches to the socket
- THEN Sirup emits a `220` ready line

### Requirement: Synthesize a ManageSieve greeting
On ManageSieve, Sirup SHALL emit the capability response the upstream last reported, followed by an `OK` completion. The greeting is the capability response on this protocol, so what an attached client reads is the real thing rather than an invented ready line.

The replayed capabilities SHALL omit `STARTTLS` and `SASL`. Neither is reachable across the socket, the connection being already encrypted and already authenticated, and advertising either invites an attached client to attempt it. `OWNER` SHALL be kept, being how a client reads back the identity the upstream settled on.

Each capability SHALL be rendered as a quoted string, the backslash and the double quote escaped and a stray CR or LF dropped, so no capability value can forge a line of its own inside the greeting.

#### Scenario: ManageSieve client attaches
- GIVEN an authenticated upstream ManageSieve session
- WHEN a client attaches to the socket
- THEN Sirup emits the post-authentication capability lines followed by `OK`, and neither `STARTTLS` nor `SASL` appears among them
