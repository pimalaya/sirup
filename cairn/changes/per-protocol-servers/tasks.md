---
cairn: tasks
id: per-protocol-servers
---

- [ ] Split `AccountConfig` into `default` plus the `imap` and `smtp` blocks
- [ ] Default the scheme per block and drop the mandatory-scheme rule
- [ ] Key the socket path on the protocol, and move `sock-file` into the block
- [ ] Take the protocols to serve as a positional list on `start`, defaulting to all
- [ ] Connect every upstream sequentially, then serve each on its own thread
- [ ] Resolve an account's secrets through one `SecretResolver`
- [ ] Take the protocol as a positional on `repl`, required beyond one block
- [ ] Generate both blocks in the wizard when discovery returns both
- [ ] Name an account declaring no block as the configuration error it is
- [ ] Update config.sample.toml, the README and himalaya's MIGRATION.md socket path
- [ ] Fold the delta into the spec and log the change
