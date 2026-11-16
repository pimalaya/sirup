---
cairn: change
id: cli-alignment-leftovers
status: landed
created: 2026-10-18
---

# What sirup still owes the family alignment

## Why

The cross-repo alignment audit of 2026-10-18 (CLI_ALIGNMENT_PLAN.md, which kept the item ids of the 2026-08-28/29 plan) found one item still open in sirup, re-checked against the code on 2026-10-18.

- **H3**: src/config.rs:336 wraps `shell_expanded_path` into a local `opt_shell_expanded_path` because pimalaya-config carried no optional twin. 0.2.1 ships it; Cargo.lock is still on 0.2.0.

pimalaya-cli will take over two local copies once its changes land: wizard-save-layer replaces `ExistingConfig`, `append_or_print` and `print_saved` (src/wizard/configure.rs:116, :247, :276), shared-account-resolution replaces `resolve_account` (src/cli.rs:306).

## What

Drop the wrapper and adopt the shared layer.
