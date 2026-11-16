---
cairn: delta
change: machine-output-contract
---

## ADDED Requirements

### Requirement: The JSON contract is described and versioned
`sirup json-schema` SHALL describe, under the key `sirup-error`, the failure object every command prints under `--json`. Every generated schema SHALL carry an `$id` naming the contract version, which is the minor version while Sirup is below 1.0 (`v0.1`) and the major version after. Every registered payload SHALL be a JSON object, checked by a test. Piped `--json` output SHALL print one value per line.

#### Scenario: A caller reads a failure against its schema
- **GIVEN** `sirup json-schema sirup-error`
- **WHEN** it runs
- **THEN** the schema of the JSON failure object is printed, its `$id` being `https://pimalaya.org/schemas/sirup/v0.1/sirup-error.json`
