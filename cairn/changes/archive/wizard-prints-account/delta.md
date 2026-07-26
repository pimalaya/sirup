---
cairn: delta
change: wizard-prints-account
---

## ADDED Requirements

### Requirement: Runs on bare invocation
Bare `sirup` with no subcommand SHALL run the wizard.

### Requirement: Prints an account fragment to stdout
The wizard SHALL print the discovered account as a `[accounts.<name>]` TOML fragment on stdout, with guidance embedded as comments and the same data serialized as an object under `--json`. Prompts render on stderr so a redirect appends the fragment to a config file. The account name is derived from the input as the table key. The wizard SHALL start no daemon.

### Requirement: Global account flag
`-a` / `--account` SHALL be a global flag on the root parser, selecting the account for whichever subcommand runs.

### Requirement: Commands resolve the account from config
`start` and `repl` SHALL resolve their account from the loaded config by the global account name, or the `default = true` account when none is given. A missing config file or an unknown account SHALL be a hard error, with no wizard fallback.

## MODIFIED Requirements

### Requirement: Never write to disk
The wizard SHALL build the account in memory and print it as a config fragment on stdout only. It SHALL NOT write credentials or configuration to disk.

## REMOVED Requirements

### Requirement: Force the wizard
