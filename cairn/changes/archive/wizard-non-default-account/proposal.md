---
cairn: change
id: wizard-non-default-account
status: landed
created: 2026-07-26
---

# Leave the wizard account non-default, like himalaya

## Why
The wizard built its account with `default = true`, which the fragment then printed. Merged into a config that already has a default account, that silently hijacks the default. Himalaya avoids this by leaving the wizard account non-default and letting the user opt in by hand.

## What
Build the wizard account with `default = false` in all three constructors. Being false, `default` is omitted from the printed fragment (the `is_false` skip), so the user marks their choice with `default = true` themselves. Updated the render test accordingly.
