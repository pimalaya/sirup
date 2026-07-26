---
cairn: log
change: wizard-non-default-account
landed: 2026-07-26
---

# Leave the wizard account non-default, like himalaya

Changed the three wizard account constructors to build with `default = false` instead of `true`. Being false, `default` is dropped from the printed fragment (the `is_false` skip), so a wizard fragment merged into a config that already has a default account no longer hijacks it; the user opts in with `default = true`. Updated the render test to assert `default` is omitted.

Spec updated: `wizard` (Account is left non-default, ADDED).
