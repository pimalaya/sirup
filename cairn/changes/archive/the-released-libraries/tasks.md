---
cairn: tasks
change: the-released-libraries
---

- [x] Require pimalaya-cli 0.6, pimalaya-config 0.5, pimalaya-stream 0.4, io-imap 0.11, io-smtp 0.9, io-managesieve 0.4 and io-sasl 0.2; drop the git pin, io-pim-discovery and nom
- [x] Delete the wizard, `configure`, its schema and the serialization it alone used
- [x] Meet a missing configuration with the shared welcome, resolve accounts through pimalaya-config
- [x] Port the connects to their options structs and `tls.cert` to `TlsServer`
- [x] README, CONTRIBUTING, config.sample.toml and CHANGELOG
- [x] Fold the delta into the spec, log, archive
