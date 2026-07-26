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
