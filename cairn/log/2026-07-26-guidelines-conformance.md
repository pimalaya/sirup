---
cairn: log
change: guidelines-conformance
landed: 2026-07-26
---

# Align code documentation with the Pimalaya guidelines

Ran a conformance pass against .github/GUIDELINES.md and closed the real gaps, all in documentation. Added module headers to config.rs, repl.rs and session.rs (inline-001). Documented every public item in config.rs (Config, AccountConfig, the TLS and SASL types with their fields and variants) plus the Session enum and its variants and the Cli account/json/log flags (inline-002, inline-006). Tagged the remaining bare inline comments with NOTE or removed the self-evident ones (inline-004), and reflowed the one over-wide doc line (inline-002). Pointed the main.rs header at cairn/ (header-001).

Minor items: set default-features = false on serde, secrecy and url to match the ortie reference (cargo-008), dropped the empty patch.crates-io block, and aligned the CONTRIBUTING cairn reference to the template without path backticks (markdown-003).

No behaviour changed, so the spec did not move; this is a documentation and manifest cleanup only. Items the ortie and himalaya references also carry (README backticks for flags/features, README semicolons, config.sample banner comments) were left as house convention. build, clippy, test, rustdoc and fmt are all clean.
