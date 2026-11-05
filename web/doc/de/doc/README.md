# fkv

fkv ist eine reine in Rust eingebettete KV-Speicher-Engine.

## Merkmal

- Ein Betriebssystem-Thread pro Shard, kooperative Arbeitsschleife.
- COW B+Baum, Segment-IO nur anhängen.
- Seiten-/Index-/Manifest-/Überlauf-/Komprimierungsformat.
- lokal/Speicher/simuliert/S3-Objekt-Backend.
- Standalone-/Cloud-/Standby-Replikatmodus.
- Kein C-ABI, kein Python-SDK; nur Rust ist die öffentliche Schnittstelle.

## Eingang

- [Architektur](architecture.md)
- [Crate](crates.md)
- [prüfen](testing.md)
- [Leistung](performance.md)