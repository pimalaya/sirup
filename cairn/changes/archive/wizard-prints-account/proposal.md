---
cairn: change
id: wizard-prints-account
status: landed
created: 2026-07-26
---

# Align the wizard and account selection with himalaya and ortie

## Why
Sirup's wizard was a config *fallback*: when no config file was found (or `--no-account` was passed), it built an account in memory and immediately started the daemon on it. Himalaya and ortie have since settled on a different, more predictable shape: bare invocation runs the wizard, which discovers an account and prints it as a ready-to-save TOML fragment on stdout without ever starting anything or writing to disk. The user redirects that output into their config. This makes the wizard a config *generator*, not a hidden runtime path, and keeps the config entirely user-owned.

Sirup should match, both for consistency across the Pimalaya CLIs and because the fallback path hid a running daemon behind an interactive prompt.

## What
- Bare `sirup` (no subcommand) runs the wizard, which discovers an account and prints it as a `[accounts.<name>]` TOML fragment on stdout (guidance embedded as comments, prompts on stderr, JSON under `--json`). It starts no daemon and writes no file.
- Remove the `--no-account` flag: bare invocation is now the way to run the wizard.
- Make `-a` / `--account` a global flag on the root parser instead of a per-command argument, matching himalaya and ortie.
- `start` and `repl` resolve their account strictly from the loaded config by the global account name (or the `default = true` account). A missing config file or unknown account is a hard error, with no wizard fallback.
