---
cairn: tasks
id: lib-bump
---

- [x] Declare io-sasl and read the credential structs from it
- [x] Rebuild the IMAP and SMTP connects around their session options structs
- [x] Reach `noop` through the `ImapClient` and `SmtpClient` traits
- [x] Downcast to `pimalaya_stream::stream::Stream` and pair non-blocking mode with `Retry::Never`
- [x] Write `Secret::Command` as a `CommandConfig`
- [x] Fold the delta into the spec and log the change
