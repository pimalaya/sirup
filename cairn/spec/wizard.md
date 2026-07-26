---
cairn: spec
capability: wizard
status: current
---

# Wizard

Bare `sirup` with no subcommand runs the wizard. It builds an account from a single input, then prints it as a ready-to-save config fragment on stdout. It starts no daemon and writes nothing to disk: the user redirects the output into their config file, so the config stays entirely user-owned. This mirrors himalaya and ortie. The wizard compiles only when both protocols and a TLS provider are enabled.

### Requirement: Runs on bare invocation
Bare `sirup` with no subcommand SHALL run the wizard.

#### Scenario: No subcommand
- GIVEN `sirup` is invoked with no subcommand
- WHEN it starts
- THEN the wizard runs

### Requirement: Prints an account fragment to stdout
The wizard SHALL print the built account as a `[accounts.<name>]` TOML fragment on stdout, with guidance embedded as comments, and the same data serialized as an object under `--json`. Prompts SHALL render on stderr so a redirect appends only the fragment. The account name SHALL be derived from the input as the table key. The wizard SHALL start no daemon.

#### Scenario: Fragment is appendable
- GIVEN the wizard has built an account from user input
- WHEN it finishes
- THEN a `[accounts.<name>]` fragment is printed on stdout while prompts stayed on stderr, so `sirup >> <config>` appends it

### Requirement: Account is left non-default
The wizard SHALL build the account with `default = false`, so it does not hijack the default when merged into a config that already has one. Being false, `default` SHALL be omitted from the printed fragment; the user opts in by adding `default = true`.

#### Scenario: Fragment omits default
- GIVEN the wizard has built an account
- WHEN it prints the fragment
- THEN no `default` key appears, and the user adds `default = true` to select it

### Requirement: Secrets use the shared picker
SASL passwords and tokens SHALL be prompted through the shared OS-aware picker (OS keyring, a custom command, or a raw value), never a bare raw-only prompt. Passwords SHALL use the keyring picker and OAuth tokens the token picker with the OAuth brokers enabled. The account name SHALL seed the keyring entry.

#### Scenario: Password prompt
- GIVEN a password mechanism (PLAIN, LOGIN or SCRAM-SHA-256)
- WHEN the wizard prompts for the password
- THEN it offers the OS keyring, a custom command, or a raw value, rather than only a raw prompt

### Requirement: Tests the account before printing
The wizard SHALL test the built account before printing it, by opening and authenticating the upstream session once and then dropping it. A connection or authentication failure SHALL abort with the error instead of printing an unusable fragment.

#### Scenario: Bad credential
- GIVEN the user entered a credential the server rejects
- WHEN the wizard tests the account
- THEN it fails with the error and prints no fragment

### Requirement: Never write to disk
The wizard SHALL build the account in memory and print it as a config fragment on stdout only. It SHALL NOT write credentials or configuration to disk.

#### Scenario: Wizard run
- GIVEN a fresh run
- WHEN the wizard builds an account from user input
- THEN the account is printed on stdout and nothing is written to disk
