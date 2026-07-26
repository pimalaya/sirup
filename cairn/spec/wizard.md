---
cairn: spec
capability: wizard
status: current
---

# In-memory wizard

The first-run wizard builds an account from a single input, holding it in memory for the current run only. This keeps throwaway sessions credential-clean and lets an operator hand off a running daemon without exposing stored secrets. The wizard compiles only when both protocols and a TLS provider are enabled.

### Requirement: Never write to disk
The wizard SHALL build an `AccountConfig` held in memory for the current run only and SHALL NOT write credentials or configuration to disk.

#### Scenario: Wizard run
- GIVEN no configuration file and a fresh run
- WHEN the wizard resolves an account from user input
- THEN the resulting `AccountConfig` exists only in memory and nothing is written to disk

### Requirement: Force the wizard
The `--no-account` flag SHALL force the wizard even when a configuration file exists.

#### Scenario: Config present but wizard forced
- GIVEN an existing configuration file
- WHEN Sirup runs with `--no-account`
- THEN the wizard runs instead of loading the file
