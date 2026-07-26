---
cairn: change
id: guidelines-conformance
status: landed
created: 2026-07-26
---

# Align code documentation with the Pimalaya guidelines

## Why
A conformance pass against .github/GUIDELINES.md found real gaps, all in code documentation rather than behaviour: three modules had no header docs, many public config fields were undocumented, and several inline comments were untagged. These are the inline-001, inline-002, inline-003, inline-004 and inline-006 rules. A few minor manifest and markdown items also drifted from the ortie reference.

## What
- Add module headers to config.rs, repl.rs and session.rs (inline-001).
- Document every public item in config.rs (the Config and AccountConfig fields, the TLS and SASL types and their fields and variants) and the Session enum and its variants; document the Cli account, json and log flags (inline-002, inline-006).
- Tag every remaining bare inline comment with NOTE, or remove it when the code is self-evident (inline-004). Reflow the one over-wide doc line (inline-002).
- Point the main.rs header at the cairn/ folder (header-001).
- Manifest and markdown: set default-features = false on serde, secrecy and url to match ortie (cargo-008), drop the empty patch.crates-io block, and align the CONTRIBUTING cairn reference to the template, dropping the path backticks (markdown-003).

No behaviour changed, so the spec does not move. Items that the ortie and himalaya references also carry (README backticks for flags and features, README semicolons, the config.sample banner comments) were deliberately left as house convention.
