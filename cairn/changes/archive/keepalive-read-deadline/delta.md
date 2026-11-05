---
cairn: delta
id: keepalive-read-deadline
---

## MODIFIED Requirements

### Requirement: Idle NOOP cadence
While idle, Sirup SHALL issue a NOOP on a four-minute cadence, chosen to sit under both the IMAP thirty-minute server-side minimum and the SMTP five-minute receiver timeout, with margin for slow round-trips. Real client traffic SHALL reset the timer, so the NOOP fires only during genuine idleness. The NOOP SHALL be armed with a read deadline above the stream's retry budget and below the cadence, so an upstream that accepts the command and never answers fails the exchange rather than parking the accept loop; the failure ends the run like any other upstream failure.
