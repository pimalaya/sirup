---
cairn: delta
change: wizard-secret-and-test
---

## ADDED Requirements

### Requirement: Secrets use the shared picker
SASL passwords and tokens SHALL be prompted through the shared OS-aware picker (OS keyring, a custom command, or a raw value), never a bare raw-only prompt. Passwords SHALL use the keyring picker and OAuth tokens the token picker with the OAuth brokers enabled. The account name SHALL seed the keyring entry.

### Requirement: Tests the account before printing
The wizard SHALL test the built account before printing it, by opening and authenticating the upstream session once and then dropping it. A connection or authentication failure SHALL abort with the error instead of printing an unusable fragment.

<!-- Confirmed, not changed: `discovery` already requires that only IMAP
and SMTP endpoints are consumed (Drop JMAP endpoints). No spec move. -->

