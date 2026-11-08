# fkv

fkv ass e pure Rust embedded KV Späichermotor.

## charakteristesch

- Ee OS thread pro Shard, kooperativ Aarbechtsschleife.
- COW B + Bam, nëmmen anzeginn Segment IO.
- page/index/manifest/overflow/compression format.
- lokal / Erënnerung / simuléiert / S3 Objet Backend.
- Standalone / Cloud / Standby Replica Modus.
- Nee C ABI, kee Python SDK; nëmmen Rust ass déi ëffentlech Interface.

## Entrée

- [Architektur](architecture.md)
- [Crate](crates.md)
- [testen](testing.md)
- [Leeschtung](performance.md)