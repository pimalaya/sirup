---
cairn: log
change: wizard-secret-and-test
landed: 2026-07-26
---

# Follow himalaya for wizard secrets and the pre-print connection test

Closed two gaps against himalaya. Secrets: added a `wizard::secret` module wrapping pimalaya-cli's keyring and token pickers (OS keyring, custom command, or raw), and routed every SASL password and token through it, seeding the keyring entry with the account name. The wizard no longer prompts for a bare raw secret. Connection test: the wizard now opens and authenticates the upstream session once (dropping it immediately) behind a "Testing account configuration" spinner before printing, and aborts on failure rather than emitting an unusable fragment. The connection resolution shared with `start` was extracted into `main::resolve_connection`, and `session::test` does the connect-and-drop.

Also confirmed, no change: the PACC/Autoconfig/SRV bricks only ever extract IMAP and SMTP endpoints (`DiscoveryResult { imap, smtp }`), dropping any JMAP a probe surfaces, since those are the only backends sirup routes. Already captured by the `discovery` spec (Drop JMAP endpoints).

Spec updated: `wizard` (Secrets use the shared picker and Tests the account before printing, both ADDED). All feature combos build, clippy, fmt and the fragment test pass.
