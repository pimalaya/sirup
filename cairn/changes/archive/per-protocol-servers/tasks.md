---
cairn: tasks
id: per-protocol-servers
---

- [x] Split `AccountConfig` into `default` plus the `imap` and `smtp` blocks
- [x] Default the scheme per block and drop the mandatory-scheme rule
- [x] Key the socket path on the protocol, and move `sock-file` into the block
- [x] Take the protocols to serve as a positional list on `start`, defaulting to all
- [x] Connect every upstream sequentially, then serve each on its own thread
- [x] Resolve an account's secrets through one `SecretResolver`
- [x] Take the protocol as a positional on `repl`, required beyond one block
- [x] Generate both blocks in the wizard when discovery returns both
- [x] Name an account declaring no block as the configuration error it is
- [x] Update config.sample.toml, the README and himalaya's MIGRATION.md socket path
- [x] Fold the delta into the spec and log the change
