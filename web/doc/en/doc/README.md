# fkv

fkv is a pure Rust embedded KV storage engine.

## Characteristic

- One OS thread per shard, cooperative work loop.
- COW B+tree, append-only segment IO.
- page/index/manifest/overflow/compression format.
- local/memory/simulated/S3 object backend.
- standalone / cloud / standby replica mode.
- No C ABI, no Python SDK; only Rust is the public interface.

## Entrance

- [Architecture](architecture.md)
- [Crate](crates.md)
- [Test](testing.md)
- [Performance](performance.md)