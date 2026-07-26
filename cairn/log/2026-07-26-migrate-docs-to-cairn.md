---
cairn: log
change: migrate-docs-to-cairn
landed: 2026-07-26
---

# Migrate docs/ to Cairn

Adopted the Cairn convention in Sirup, the first Pimalaya repository to do so. Folded the settled design from `docs/design.md` into five current-truth capability specs under `cairn/spec/`: `socket-proxy`, `greeting`, `keepalive`, `discovery`, and `wizard`. Added the activation surface (`AGENTS.md` with `CLAUDE.md`, Cursor, and Copilot pointers). Removed the `docs/` folder and repointed `CONTRIBUTING.md` at `cairn/`.

Spec updated: `socket-proxy`, `greeting`, `keepalive`, `discovery`, `wizard` (all ADDED). The pre-Cairn design rationale (rejected alternatives) was not copied into the spec, which holds current truth only; it stays retrievable in git history at `docs/design.md`. The crate architecture remains documented in the `src/main.rs` header.
