---
cairn: tasks
change: wizard-prints-account
---

- [x] Derive `Serialize` on the account config types and skip the `false` bools
- [x] Wizard `run(printer)` derives a name and prints a TOML fragment; drop the daemon start
- [x] Make `-a`/`--account` a global root flag; drop per-command account args
- [x] Bare invocation runs the wizard; remove `--no-account`
- [x] `start`/`repl` load the account from config (hard error if missing); drop `load_or_wizard`/`default_for_wizard`
- [x] Update `src/main.rs` header, `config.sample.toml`, README/CHANGELOG as needed
- [x] `cargo fmt` + build all feature combos; fold spec and log
