---
cairn: change
id: first-run-contract
status: landed
created: 2026-08-29
---

# Meet the first-run contract

## Why

The CLI alignment plan measures the nine Pimalaya binaries against guidelines cli-001 to cli-007, and sirup is the one that breaches the most of them. It is also the only binary of the nine whose wizard cannot be reached by name, and the only one that prompts a cron job.

- cli-002: `take_account` bails a bare "Cannot find account" from main.rs. A mistyped `-a` and an unset default read the same, and neither names a way out. A missing configuration file names no path, so a mistyped `-c` shows up as a generic first run.
- cli-003: there is no `configure` command. The wizard is reachable only by running sirup with no subcommand, which is also what a newcomer types to see the help.
- cli-004 and cli-005: a bare `sirup` runs the wizard unconditionally, with no welcome, no offer and no help fallback, and it never offers to save what it generated. The generated account goes to stdout and the user is told to redirect it.
- cli-006: sirup calls `is_terminal` nowhere, so it prompts when stdin is a pipe and mixes its banner into a redirected document.
- cli-007: there is no welcome, and `--help` carries neither the bug tracker nor the sponsoring links.

Two of these also break the shell-expansion policy the same plan asks for (item B1): `socks-dir`, `sock-file` and `tls.cert` are read as written, so `socks-dir = "~/run"` binds a socket under a literal `./~/run`.

## What

Give sirup the same first-run contract the other eight carry, without waiting for the shared wizard save layer the plan wants pimalaya-cli to own (item H1): the layer is written here in the shape the seven other copies have, so moving it up later is a deletion rather than a redesign.

- Move the parser and the account resolution out of main.rs into a cli module, leaving main.rs a frontend that meets the bare invocation.
- Name the three configuration failures, listing the accounts a configuration does hold and both ways of picking a default.
- Add a `configure` command running the wizard on demand, and make the bare invocation offer it when there is no configuration and print the help otherwise.
- Write the generated account: a missing file whole, an existing one appended to as plain text, guarded by the two invariants the shared accounts table imposes.
- Decide interactivity from the streams, and carry the `footer!` links in `--help`.
- Shell-expand every path in the schema at deserialize time, so no call site can forget.
- Give the generated account an output type deriving `Display`, `Serialize` and `JsonSchema`, and a `json-schema` command to publish it (item B9).
