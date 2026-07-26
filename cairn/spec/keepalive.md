---
cairn: spec
capability: keepalive
status: current
---

# Keepalive

The accept loop is non-blocking and polls with a short timeout so it can interleave two duties: accepting a new client and keeping the upstream session warm while idle. This prevents the upstream server from dropping an authenticated session that no client is currently using.

### Requirement: Idle NOOP cadence
While idle, Sirup SHALL issue a NOOP on a four-minute cadence, chosen to sit under both the IMAP thirty-minute server-side minimum and the SMTP five-minute receiver timeout, with margin for slow round-trips. Real client traffic SHALL reset the timer, so the NOOP fires only during genuine idleness.

#### Scenario: Session kept warm while idle
- GIVEN a running daemon with no attached client
- WHEN four minutes elapse without traffic
- THEN Sirup issues a NOOP to the upstream session and the session stays authenticated

#### Scenario: Client traffic defers the NOOP
- GIVEN a running daemon
- WHEN a client sends traffic before the cadence elapses
- THEN the keepalive timer is reset and no idle NOOP is issued
