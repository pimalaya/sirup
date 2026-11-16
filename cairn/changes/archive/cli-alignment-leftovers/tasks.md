---
cairn: tasks
change: cli-alignment-leftovers
---

- [x] H3: bump pimalaya-config to 0.2.1 and use its `opt_shell_expanded_path`, dropping the local wrapper (src/config.rs:336); done by the-released-libraries on pimalaya-config 0.5
- [x] H1, H2: once pimalaya-cli ships them, drop src/wizard/configure.rs:116, :247, :276 and src/cli.rs:306; lapsed with the wizard (the-released-libraries), `resolve_account` now taking pimalaya-config's `TomlConfig::take_account`
- [x] CHANGELOG under [Unreleased]: covered by the-released-libraries' entries
- [x] Fold into the spec; log; land; move the folder to cairn/changes/archive/
