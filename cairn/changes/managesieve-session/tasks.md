---
cairn: tasks
change: managesieve-session
---

- [ ] Bump pimalaya-stream to 0.3 and replace the `StreamStd` downcasts in session.rs with `Stream`
- [ ] Bump io-imap to 0.6 and io-smtp to 0.3, rewriting the connect path onto their session coroutines
- [ ] Bump io-pim-discovery and pimalaya-cli, keeping the wizard behaviour unchanged
- [ ] Verify IMAP and SMTP still start, proxy, keepalive and pass the wizard test after the sweep
- [ ] Add the `managesieve` cargo feature and the io-managesieve dependency
- [ ] Add `Session::Managesieve`, its `connect`, its `noop` and its stream controls
- [ ] Synthesize the ManageSieve greeting from the post-authentication capabilities, dropping `STARTTLS` and `SASL`
- [ ] Accept `sieve://` and `sieves://` in the account URL, alongside the existing schemes
- [ ] Check a real errand end to end: `himalaya sieve list` against the bound socket with no credentials configured
- [ ] Offer the PACC-discovered ManageSieve endpoint in the wizard, and prompt by hand otherwise
- [ ] Update the README, config.sample.toml and CHANGELOG, then fold the delta and write the log
