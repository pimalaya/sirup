# Sirup docs

Living design documents and plans for Sirup. The repository architecture itself is documented in the src/main.rs header, the same way lib.rs documents the io- libraries.

[design.md](./design.md) records the settled design: why Sirup is a socket-proxy daemon, how it replaces the protocol greeting and keeps the upstream session alive, why it downcasts to a single concrete stream type, the order of the discovery chain, and the alternatives that were rejected.
