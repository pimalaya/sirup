---
cairn: tasks
id: keepalive-read-deadline
---

- [x] Replace the `set_read_timeout(None)` before the keepalive NOOP with a bounded `KEEPALIVE_TIMEOUT`
- [x] Fold the delta into the keepalive spec and log the change
