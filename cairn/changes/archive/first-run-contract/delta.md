---
cairn: delta
id: first-run-contract
---

## ADDED Requirements

- configuration: the three resolution failures each name what is missing and what to do about it.
- configuration: every path field is shell-expanded as it is deserialized.
- wizard: `sirup configure` runs the wizard on demand, and is the only entry point skipping the welcome.
- wizard: the generated account is written to a configuration that does not exist yet, appended as plain text to one that does, or printed when either is declined.
- wizard: the account name is taken until free, and the generated account claims the default only when no other account does.
- wizard: prompts are skipped when stdin is not a terminal or `--json` is set, and the document goes to stdout when stdout is redirected.
- wizard: a bare `sirup` offers the wizard when it finds no configuration and prints the help otherwise.

## MODIFIED Requirements

- wizard / "Runs on bare invocation": a bare `sirup` no longer runs the wizard unconditionally.
- wizard / "Prints an account fragment to stdout": printing is now the fallback, not the only outcome.
- wizard / "Account is left non-default": the account claims the default when no other one does.
- wizard / "Never write to disk": the wizard writes the configuration the user accepted.
- socket-proxy / "Commands resolve the account from config": a missing configuration raises the offer before it errors.
