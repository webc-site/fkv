# fkv

Ang fkv usa ka purong Rust nga naka-embed nga KV storage engine.

## kinaiya

- Usa ka OS thread kada shard, kooperatiba nga work loop.
- COW B+ tree, idugtong-lamang nga bahin IO.
- panid/index/manifest/overflow/compression format.
- local/memorya/simulated/S3 object backend.
- standalone / cloud / standby replica mode.
- Walay C ABI, walay Python SDK; Ang Rust lamang ang interface sa publiko.

## Entrance

- [Arkitektura](architecture.md)
- [Crate](crates.md)
- [pagsulay](testing.md)
- [pasundayag](performance.md)