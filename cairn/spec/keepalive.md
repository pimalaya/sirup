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

### Requirement: The ManageSieve keepalive is tagged
The ManageSieve `NOOP` SHALL carry a tag, which the server echoes back in the `TAG` response code, so a keepalive reply is distinguishable from anything else arriving on the stream. An echo that does not match SHALL end the run rather than leave a desynchronised session to be proxied to the next client. The thirty-minute ManageSieve inactivity minimum sits above the shared four-minute cadence, so no separate one is needed.

#### Scenario: Keepalive reply is identified
- GIVEN a running ManageSieve daemon with no attached client
- WHEN the idle cadence elapses and Sirup issues its tagged `NOOP`
- THEN the server echoes the tag, the session stays authenticated, and a reply carrying any other tag ends the run
