---
cairn: delta
change: migrate-docs-to-cairn
---

## ADDED Requirements

### Requirement: Authenticate once, proxy bytes
`sirup start` authenticates the upstream session once, then proxies raw bytes between an attached Unix-socket client and the upstream stream.

### Requirement: One long-lived instance per account
Sirup runs as a long-lived daemon, one instance per account, so connect and authentication cost is paid once.

### Requirement: Single concrete stream downcast
Sirup downcasts the boxed upstream stream to the concrete `StreamStd` to set read timeouts; the downcast is infallible by construction.

### Requirement: Synthesize a pre-authenticated greeting
Sirup emits an untagged IMAP `PREAUTH` greeting or an SMTP `220` line per attaching client instead of forwarding the consumed upstream greeting.

### Requirement: Idle NOOP cadence
Sirup issues a keepalive NOOP on a four-minute idle cadence, reset by real client traffic.

### Requirement: Fixed probe order, first hit wins
The wizard probes PACC, then Autoconfig, then RFC 6186 SRV, taking the first non-empty result.

### Requirement: Direct URL skips discovery
A direct `imap` or `smtp` URL skips discovery entirely.

### Requirement: Drop JMAP endpoints
Any JMAP endpoint a probe surfaces is dropped, since Sirup routes only IMAP and SMTP.

### Requirement: Configurable resolver
The DNS resolver is selected from `SIRUP_DNS_RESOLVER`, then the system resolver, before any public default.

### Requirement: Never write to disk
The wizard builds an in-memory `AccountConfig` for the current run only and writes nothing to disk.

### Requirement: Force the wizard
The `--no-account` flag forces the wizard even when a configuration file exists.
