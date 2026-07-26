---
cairn: log
change: wizard-prints-account
landed: 2026-07-26
---

# Align the wizard and account selection with himalaya and ortie

Reshaped the wizard and account selection to match himalaya and ortie. Bare `sirup` (no subcommand) now runs the wizard, which discovers an account and prints it as a `[accounts.<name>]` TOML fragment on stdout (guidance as comments, prompts on stderr, the same data under `--json`), deriving the table key from the input. It starts no daemon and writes nothing to disk. The `--no-account` flag is gone, and `-a` / `--account` moved to the root parser as a global flag. `start` and `repl` now resolve their account strictly from the loaded config, a missing config file or unknown account being a hard error with no wizard fallback.

Implementation: the account config types gained `Serialize` (with the two `false` bools skipped) so `pimalaya_config::toml::to_string` renders the fragment; `wizard::discover::run` now takes a printer and emits a `GeneratedConfig` newtype; `load_or_wizard` and `Config::default_for_wizard` were replaced by a config-only `take_account`. A unit test covers the rendered fragment. All feature combos build, clippy and fmt clean.

Spec updated: `wizard` (Never write to disk MODIFIED; Runs on bare invocation and Prints an account fragment to stdout ADDED; Force the wizard REMOVED) and `socket-proxy` (Global account flag and Commands resolve the account from config ADDED).
