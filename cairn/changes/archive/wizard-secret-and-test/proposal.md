---
cairn: change
id: wizard-secret-and-test
status: landed
created: 2026-07-26
---

# Follow himalaya for wizard secrets and the pre-print connection test

## Why
Two gaps remained after the wizard was reshaped to print a config fragment. First, it prompted for passwords and tokens as raw values, so the printed fragment carried plaintext secrets. Himalaya instead routes secrets through a shared OS-aware picker (OS keyring, a custom command, or raw as a last resort), keeping secrets out of the config by default. Second, the wizard printed the account without ever contacting the server, so a bad credential or endpoint only surfaced later, on `start`. Himalaya tests the account first and refuses to print one that cannot connect.

## What
- Route every SASL password and token through a new `wizard::secret` module that wraps pimalaya-cli's keyring/token pickers, exactly like himalaya's `wizard::secret`. Passwords use the keyring picker, OAuth tokens the token picker (OAuth brokers enabled). The account name seeds the keyring entry. Raw stays available but is no longer the only option.
- Test the built account before printing it: open and authenticate the upstream session once (dropping it immediately), behind a "Testing account configuration" spinner. A failure aborts with the error instead of printing an unusable fragment. The connection resolution shared by `start` is extracted into `resolve_connection`, and `session::test` performs the connect-and-drop.

Also confirmed (no code change): the discovery bricks (PACC, Autoconfig, SRV) only ever extract IMAP and SMTP endpoints; any JMAP a probe surfaces is dropped at the source, since those are the only backends sirup routes.
