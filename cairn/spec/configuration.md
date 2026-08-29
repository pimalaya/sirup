---
cairn: spec
capability: configuration
status: current
---

# Configuration

Sirup reads a TOML document holding named accounts, from the first canonical path or the one `-c` overrides. Resolving an account is where a command discovers it has nothing to run against, and each of the three ways that fails names what is missing and what to do about it.

### Requirement: A missing configuration names the path
A command finding no configuration file SHALL name the path it looked for, which is the one `-c` gave or the default location, and SHALL name both `sirup configure` and the documented sample as the two ways out.

#### Scenario: Mistyped `-c`
- GIVEN `sirup -c /tmp/nope.toml start`
- WHEN the configuration is resolved
- THEN the error names /tmp/nope.toml, so the mistyped path shows up as itself rather than as a generic first run

### Requirement: An unknown account lists the ones that exist
A command given an `-a` name the configuration does not hold SHALL list the account names it does hold, sorted.

#### Scenario: Typo in `-a`
- GIVEN a configuration holding `work` and `perso`
- WHEN `sirup -a wrok start` runs
- THEN the error names `wrok` and lists `perso, work`

### Requirement: A missing default names both ways of picking one
A command running without `-a` against a configuration where no account claims `default` SHALL name both `-a <NAME>` and `default = true`.

#### Scenario: No account claims the default
- GIVEN a configuration whose accounts all leave `default` unset
- WHEN `sirup start` runs
- THEN the error names `-a <NAME>` and `default = true`

### Requirement: A missing configuration raises the offer first
Before it errors, a command needing an account SHALL offer to generate a configuration when it finds none. The offer is a hook rather than a gate: the command carries on afterwards either way, so accepting gives it a chance to work and declining leaves it to fail on the configuration it still has not got.

#### Scenario: First run of a command
- GIVEN no configuration file on disk and a terminal on the standard input
- WHEN `sirup start` runs
- THEN the welcome is printed and the wizard is offered, and the command carries on with whatever configuration exists afterwards

### Requirement: Every path is shell-expanded at deserialize
`socks-dir`, `sock-file` and `tls.cert` SHALL be expanded as they are deserialized, environment variables and the leading tilde alike, so no call site reads one as written and no new path field can forget to expand.

#### Scenario: Tilde in the sockets directory
- GIVEN `socks-dir = "~/run"`
- WHEN the configuration is read
- THEN the value is the absolute path under the home directory, not a relative one under a directory named `~`
