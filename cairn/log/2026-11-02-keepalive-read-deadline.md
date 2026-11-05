---
cairn: log
id: keepalive-read-deadline
date: 2026-11-02
---

# Gave the keepalive NOOP a read deadline

The accept loop cleared the socket timeout (`set_read_timeout(None)`) immediately before issuing the idle NOOP, making it the one read in the daemon without a bound. Against a half-open TCP connection -- a middlebox acknowledging for a backend that is gone -- the write succeeds, the tagged reply never arrives, and the blocking read parks forever: the session is dead, the process is not, and the failure-ends-the-run contract that supervisors rely on never fires.

The shape is invisible from everywhere else: the local socket keeps greeting PREAUTH behind the dead session, and the upstream connection still reports established with an empty queue and no retransmissions, because the middlebox acknowledges the request bytes on the gone backend's behalf. Only an application probe with its own deadline can separate a stalled peer from a slow one.

`serve_one` now arms `KEEPALIVE_TIMEOUT` (ninety seconds) before the NOOP: above the stream's retry budget, so the first socket timeout ends the exchange, and well under the four-minute cadence, so a slow reply still lands. A stalled upstream now fails the NOOP and ends the run for its supervisor to restart.

Capabilities moved: keepalive.
