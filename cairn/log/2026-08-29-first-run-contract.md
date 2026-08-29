---
cairn: log
id: first-run-contract
date: 2026-08-29
---

# Met the first-run contract

Sirup now carries the same first-run contract as the other eight Pimalaya binaries, measured against guidelines cli-001 to cli-007.

`Cli`, `Command` and the account resolution moved into a cli module, leaving main.rs a frontend that meets the bare invocation and nothing else. A bare `sirup` no longer runs the wizard: it offers one when it finds no configuration, and prints the help otherwise, a declined offer falling back to the help since there is nothing to carry on to. `sirup configure` is the wizard reached by name, and the only entry point skipping the welcome.

The three configuration failures each name what is missing. A missing file names the path it looked for, which is the one `-c` gave; an unknown `-a` name lists the accounts the configuration does hold, sorted; a missing default names both `-a <NAME>` and `default = true`. All three used to be a bare "Cannot find account".

The wizard writes what it generates. It hands back the account and the name it suggests, and the configure command decides: a file that does not exist yet is written whole, one that exists is appended to as plain text so its comments and formatting survive, and either offer declined prints the document instead. Two invariants guard the append: the name is suffixed until free, and the account claims `default` only when no other one does. Nothing prompts when the standard input is not a terminal or `--json` is set, and the document goes to the standard output whenever the standard output is redirected.

Two items rode along from the alignment plan. `socks-dir`, `sock-file` and `tls.cert` are shell-expanded as they are deserialized, so `socks-dir = "~/run"` no longer binds a socket under a directory named `~`. The generated account got a `ConfigureOutput` type deriving `Display`, `Serialize` and `JsonSchema`, published by a new `json-schema` command.

The discovery probes are gated on a `discovery` cfg the build script sets from the imap, smtp and TLS features, collapsing the three-line feature list that used to be repeated at four call sites. `configure` itself is always compiled and says what is missing when it cannot run.

The wizard save layer is written here in the shape the seven other copies have, so moving it up into pimalaya-cli later is a deletion rather than a redesign.

Capabilities moved: wizard, configuration (new), socket-proxy.
