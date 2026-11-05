---
cairn: change
id: keepalive-read-deadline
status: landed
created: 2026-11-02
---

# Give the keepalive NOOP a read deadline

## Why

The idle keepalive is the daemon's only test of whether the upstream session is still alive, and it is the one read in the whole process that was deliberately left without a deadline: the accept loop calls `set_read_timeout(None)` right before the NOOP. Every other guard against a dead session (the socket timeout, the retry budget) is cleared by that line.

The shape it cannot survive is a half-open TCP connection: a middlebox that keeps the connection and acknowledges bytes on behalf of a backend that is already gone. The NOOP write succeeds, the tagged reply never comes, and the blocking read parks forever. The spec promises that the first upstream failure ends the run so a supervisor can restart the session; a keepalive that never returns breaks that promise without raising any error, and the daemon keeps serving a dead session on its socket forever.

The stall hides from every other vantage point. The socket keeps greeting PREAUTH behind the dead session, so clients only notice when a command needs an upstream reply. The transport layer has nothing to report: the connection stays established with nothing queued and no retransmissions, precisely because the middlebox acknowledges for the vanished backend. Only a probe carrying its own read deadline can tell a stalled session from a merely slow one, and that probe is the keepalive NOOP -- the one read that had no deadline.

## What

Arm a deadline on the stream before the keepalive NOOP instead of clearing it: `KEEPALIVE_TIMEOUT`, above the stream's retry budget so the first socket timeout ends the exchange, and well under the four-minute cadence so a live-but-slow reply is never mistaken for a stalled one. A stalled upstream now fails the NOOP and ends the run like any other upstream failure, leaving the restart to the supervisor.
