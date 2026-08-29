---
cairn: delta
id: lib-bump
---

## MODIFIED Requirements

- socket-proxy / "Single concrete stream downcast": the concrete type is `pimalaya_stream::stream::Stream`, not `StreamStd`.

## ADDED Requirements

- socket-proxy: non-blocking mode and the stream retry strategy are set together, so an idle pass surfaces `WouldBlock` instead of being retried away.
