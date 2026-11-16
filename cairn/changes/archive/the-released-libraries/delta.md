---
cairn: delta
change: the-released-libraries
---

## ADDED Requirements

### Requirement: A person missing a configuration meets the welcome
A bare `sirup`, or a command needing an account, finding no configuration file SHALL fail naming the path it looked for and the documented sample. When a person reads the output (no `--json`, stderr a terminal) it SHALL print pimalaya-cli's welcome instead: the commands creating the file from the shipped sample, or downloading it when none ships, then `sirup start`. Nothing offers to write a configuration.

#### Scenario: First run at a terminal
- GIVEN no configuration file on disk and a terminal on stderr
- WHEN `sirup start` runs
- THEN the welcome names the missing path, the command creating it from the sample and then `sirup start`, and the command exits 1

#### Scenario: First run in a script
- GIVEN no configuration file on disk and `--json`
- WHEN `sirup --json start` runs
- THEN the failure object names the missing path and the sample, and the command exits 1

### Requirement: Several defaults are refused
A command running without `-a` against a configuration where several accounts claim `default` SHALL fail naming them, rather than pick one.

## MODIFIED Requirements

### Requirement: A missing configuration names the path
A command finding no configuration file SHALL name the path it looked for, which is the one `-c` gave or the default location, and the documented sample as the way out.

### Requirement: An unknown account lists the ones that exist
A command given an `-a` name the configuration does not hold SHALL list the account names it does hold, sorted. `default` is a name like any other: `-a default` takes the account named `default`, an absent or empty `-a` the one marked default.

### Requirement: Commands resolve the account from config
`start` and `repl` SHALL resolve their account from the loaded config by the global account name, or the `default = true` account when none is given. Each way that fails is a hard error naming what is missing, described under the configuration capability, and a missing configuration meets a person with the welcome instead.

## REMOVED Requirements

- configuration / "A missing configuration raises the offer first": nothing offers to generate a configuration.
- wizard: the whole capability (Reachable by name; A bare invocation offers, then falls back to the help; The name is derived, and taken until free; The default is claimed only when free; Saving never rewrites what a human wrote; Interactivity is decided by the streams; Secrets use the shared picker; Tests every block before handing the account back; Discovery keeps every endpoint it finds; The welcome frames the product).
- discovery: the whole capability (Fixed probe order, first hit wins; Direct URL skips discovery; Drop JMAP endpoints; Configurable resolver).
