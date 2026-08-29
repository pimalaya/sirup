---
cairn: spec
capability: wizard
status: current
---

# Wizard

The wizard generates an account, it never edits one. It builds one from a single input, tests it, and hands back a ready-to-place `[accounts.<name>]` table. What becomes of that table is its own half: a configuration file that does not exist yet is written whole, one that exists is appended to as plain text, and a declined offer prints it instead. Editing an account, adding a second one by hand and everything the questions do not cover belong to the file and the user's editor, against config.sample.toml. The discovery probes compile only when both protocols and a TLS provider are enabled; the command exists whatever the feature set and says what is missing when it cannot run.

### Requirement: Reachable by name
`sirup configure` SHALL run the wizard on demand, and is the only entry point skipping the welcome, having been asked for by name.

#### Scenario: Configure command
- GIVEN a configuration that already exists
- WHEN `sirup configure` runs
- THEN the wizard runs with no welcome, and offers to append the account it generates

### Requirement: A bare invocation offers, then falls back to the help
A bare `sirup` SHALL offer the wizard when it finds no configuration, and print the help otherwise. A bare invocation has nothing to carry on to, so a declined offer falls back to the help. A configuration that fails to parse counts as a configuration, so the offer never writes over one, and `--account` alone is a half-typed command rather than a first run.

#### Scenario: First run
- GIVEN no configuration file on disk and a terminal on the standard input
- WHEN `sirup` runs with no subcommand
- THEN the welcome is printed and the wizard is offered, and declining prints the help

### Requirement: The name is derived, and taken until free
The account name SHALL be derived from the input rather than prompted, being only the table key, and SHALL be suffixed until the configuration does not hold it already. A second `[accounts.<name>]` table of one name makes the whole document fail to parse, taking the accounts that used to work down with it.

#### Scenario: Name already taken
- GIVEN a configuration holding `example` and `example-2`
- WHEN the wizard derives `example` from the input
- THEN the generated table is `[accounts.example-3]`

### Requirement: The default is claimed only when free
The generated account SHALL claim `default` only when no other account does. Two defaults resolve to whichever one the account map yields first.

#### Scenario: An account already holds the default
- GIVEN a configuration whose `work` account sets `default = true`
- WHEN the wizard generates a second account
- THEN that account leaves `default` unset, and the report names the `-a` flag reaching it

### Requirement: Saving never rewrites what a human wrote
A configuration file that does not exist yet SHALL be written whole. One that exists SHALL be appended to as plain text, never parsed and re-serialized, so its comments, its ordering and its formatting survive.

#### Scenario: Appending to a hand-written configuration
- GIVEN a configuration opening with a comment and holding one account
- WHEN the wizard appends a second account
- THEN the comment, the ordering and the formatting are untouched, and both accounts parse back

### Requirement: Interactivity is decided by the streams
Nothing SHALL prompt when the standard input is not a terminal or when `--json` is set: both get the error that names the way out. The generated document SHALL go to the standard output whenever the standard output is redirected, and every prompt, banner and confirmation SHALL render on the standard error so it never pollutes that document.

#### Scenario: Redirected wizard
- GIVEN a terminal on the standard input and a redirected standard output
- WHEN `sirup configure > config.toml` runs
- THEN the prompts render on the standard error, the document lands in the file, and nothing else is written to disk

#### Scenario: Cron job
- GIVEN the standard input is a pipe
- WHEN `sirup configure` runs
- THEN it fails naming the documented sample, rather than waiting on a prompt nothing will answer

### Requirement: Secrets use the shared picker
SASL passwords and tokens SHALL be prompted through the shared OS-aware picker (OS keyring, a custom command, or a raw value), never a bare raw-only prompt. Passwords SHALL use the keyring picker and OAuth tokens the token picker with the OAuth brokers enabled. The account name SHALL seed the keyring entry.

#### Scenario: Password prompt
- GIVEN a password mechanism (PLAIN, LOGIN or SCRAM-SHA-256)
- WHEN the wizard prompts for the password
- THEN it offers the OS keyring, a custom command, or a raw value, rather than only a raw prompt

### Requirement: Tests the account before handing it back
The wizard SHALL test the built account before handing it back, by opening and authenticating the upstream session once and then dropping it. A connection or authentication failure SHALL abort with the error instead of generating a table that cannot connect.

#### Scenario: Bad credential
- GIVEN the user entered a credential the server rejects
- WHEN the wizard tests the account
- THEN it fails with the error and generates nothing

### Requirement: The welcome frames the product
The welcome a first run prints SHALL frame Sirup in a sentence, name the configuration file that is missing, say what the wizard covers and what stays hand-written, link config.sample.toml, and mention that `configure` runs the same wizard later so declining costs nothing. The `--help` footer SHALL carry the bug tracker and the sponsoring links.

#### Scenario: Welcome before the offer
- GIVEN a first run raising the offer
- WHEN the welcome is printed
- THEN it names the missing path and the sample, and says `sirup configure` runs the wizard again later
