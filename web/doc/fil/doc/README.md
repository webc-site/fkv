# fkv

Ang fkv ay isang purong Rust na naka-embed na KV storage engine.

## katangian

- Isang OS thread bawat shard, cooperative work loop.
- COW B+tree, idugtong-lamang na segment na IO.
- page/index/manifest/overflow/compression na format.
- local/memory/simulate/S3 object backend.
- standalone / cloud / standby replica mode.
- Walang C ABI, walang Python SDK; Tanging ang Rust ang pampublikong interface.

## Pagpasok

- [Arkitektura](architecture.md)
- [Crate](crates.md)
- [pagsubok](testing.md)
- [pagganap](performance.md)