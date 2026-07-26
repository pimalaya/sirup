---
cairn: tasks
change: wizard-secret-and-test
---

- [x] Add `wizard::secret` (keyring/token/command/raw), mirroring himalaya
- [x] Route SASL passwords and tokens through it; thread the account name as the keyring key
- [x] Extract `resolve_connection`; add `session::test` (connect and drop)
- [x] Test the account before printing, behind a spinner; abort on failure
- [x] Confirm discovery only yields IMAP/SMTP
- [x] Build all feature combos + clippy + fmt; fold spec and log
