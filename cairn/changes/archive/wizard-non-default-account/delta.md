---
cairn: delta
change: wizard-non-default-account
---

## ADDED Requirements

### Requirement: Account is left non-default
The wizard SHALL build the account with `default = false`, so it does not hijack the default when merged into a config that already has one. Being false, `default` SHALL be omitted from the printed fragment; the user opts in by adding `default = true`.
