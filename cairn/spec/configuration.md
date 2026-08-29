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

### Requirement: One server per protocol
An account SHALL declare one server block per protocol it speaks, `imap` and `smtp`, each carrying that protocol's own server address, socket override, TLS profile, STARTTLS switch, ALPN override and SASL mechanism. An account is a mailbox rather than a session, which is the shape every other Pimalaya binary reads.

#### Scenario: One mailbox, two protocols
- GIVEN a provider serving both IMAP and SMTP
- WHEN the user configures it
- THEN one `[accounts.<name>]` table declares both blocks, rather than two accounts declaring one server each

### Requirement: An account declaring no block is named
An account declaring neither block SHALL be refused naming the blocks it could declare and the documented sample, rather than serving nothing.

#### Scenario: Empty account
- GIVEN an account carrying only `default = true`
- WHEN `sirup start` runs against it
- THEN it fails naming the `imap` and `smtp` blocks and the sample

### Requirement: The scheme is optional
A block's `server` SHALL accept a bare authority, taking the protocol's implicit-TLS scheme (`imaps://`, `smtps://`), and a full URL verbatim. A scheme the block's protocol does not speak SHALL be rejected naming the ones it does.

#### Scenario: Bare authority
- GIVEN `imap.server = "imap.example.com"`
- WHEN the connection is resolved
- THEN it reaches `imaps://imap.example.com:993`

#### Scenario: Scheme of another protocol
- GIVEN `smtp.server = "imaps://mail.example.com"`
- WHEN the connection is resolved
- THEN it fails naming `smtp.server` and the schemes SMTP speaks

### Requirement: One secret resolver per account
An account SHALL resolve every secret its blocks name through a single memoizing resolver, so one credential command named by two blocks is spawned once. The resolver holds the plaintext for its lifetime, so it SHALL be dropped once every session is open.

#### Scenario: Both blocks name one entry
- GIVEN an account whose `imap` and `smtp` blocks both name `["pass", "show", "fastmail"]`
- WHEN `sirup start` opens both sessions
- THEN `pass` is spawned once, so a locked key is unlocked a single time
