# fkv

fkv är en ren Rust-inbäddad KV-lagringsmotor.

## karakteristisk

- En OS-tråd per skärva, samarbetsslinga.
- COW B+träd, endast tilläggssegment IO.
- sida/index/manifest/overflow/komprimeringsformat.
- lokal/minne/simulerad/S3-objekt backend.
- fristående / moln / standby replika läge.
- Ingen C ABI, ingen Python SDK; endast Rust är det offentliga gränssnittet.

## Ingång

- [Arkitektur](architecture.md)
- [Crate](crates.md)
- [testa](testing.md)
- [prestanda](performance.md)