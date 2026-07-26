---
cairn: change
id: migrate-docs-to-cairn
status: landed
created: 2026-07-26
---

# Migrate docs/ to Cairn

## Why
Sirup kept its living design in a `docs/` folder (a `design.md` of settled decisions plus a thin `README.md`). That is the retrospective habit Cairn is meant to replace with a current-truth spec, reviewable change proposals, and a dated log. Sirup is the first Pimalaya repository to adopt Cairn, so it doubles as the pilot.

## What
Fold the settled design in `docs/design.md` into per-capability spec files under `cairn/spec/`: `socket-proxy`, `greeting`, `keepalive`, `discovery`, and `wizard`. Add the Cairn activation surface (`AGENTS.md` with `CLAUDE.md`, Cursor, and Copilot pointers). Record this migration as the first change and log entry. Remove the now-empty `docs/` folder and repoint `CONTRIBUTING.md` at `cairn/`.

The `docs/` prose carried design *rationale* (alternatives that were weighed and rejected). Cairn spec files hold current truth only, with no rationale, so that rationale is not copied forward. It remains retrievable in git history at `docs/design.md`. The repository architecture stays documented in the `src/main.rs` header, unchanged.
