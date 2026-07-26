# AGENTS.md: Cairn activation

This repository follows **Cairn**, a language- and tool-agnostic convention for keeping a living spec, reviewable change proposals, and an honest history. The convention is defined by the [pimalaya/cairn](https://github.com/pimalaya/cairn) repository (`CAIRN.md` for the format, `GUIDE.md` for the procedure). No tooling is required. You create and check the structure by reading and following the rules below.

If you are an agent working in this repository, do the following **by default, without being asked**.

## 1. Locate the Cairn root

The Cairn root is the `cairn/` directory at the repository root. All Cairn artifacts live under it: `spec/` (current truth, one file per capability), `changes/` (in-flight proposals), and `log/` (dated history).

## 2. Before non-trivial work, propose

For anything beyond a trivial fix, create `cairn/changes/<change-id>/` with:

- `proposal.md`: *why* and *what* (frontmatter: `cairn: change`, `id`, `status: active`, `created`).
- `tasks.md`: the checklist.
- `delta.md`: what this changes in the spec (`ADDED`, `MODIFIED`, `REMOVED`).

Let the human review intent **before** you write code. Trivial fixes may skip this and go straight to landing.

## 3. After work lands, fold and log (never skip)

- Fold the change's delta into `cairn/spec/<capability>.md` so the spec always reflects current truth (append ADDED, replace MODIFIED, delete REMOVED).
- Append a dated entry `cairn/log/YYYY-MM-DD-<change-id>.md` describing what landed and which capabilities moved.
- Set the change `status: landed` and optionally move its folder to `cairn/changes/archive/`.

> **The forcing rule:** a change that affects behaviour is not *done* until the spec is updated and the log entry is written.

## 4. Stay conformant

Check the structure yourself: a `cairn/` root with `spec/ changes/ log/`, every Cairn file carrying a valid `cairn:` type (`spec`, `change`, `tasks`, `delta`, `log`), each change having `proposal.md` and `tasks.md`, kebab-case ids, literal delta headings (`## ADDED Requirements`, `## MODIFIED Requirements`, `## REMOVED Requirements`), dated log files `YYYY-MM-DD-<id>.md`, and a log entry for every landed change. Everything else (prose, naming, ordering, extra files) is free.
