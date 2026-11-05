# fkv

fkv er en ren Rust-indlejret KV-lagringsmotor.

## karakteristisk

- Én OS-tråd pr. skår, samarbejdende arbejdsløkke.
- COW B+træ, kun vedhæftede segment IO.
- side/indeks/manifest/overløb/komprimeringsformat.
- lokal/hukommelse/simuleret/S3 objekt backend.
- standalone / sky / standby replika-tilstand.
- Ingen C ABI, ingen Python SDK; kun Rust er den offentlige grænseflade.

## Indgang

- [Arkitektur](architecture.md)
- [Crate](crates.md)
- [prøve](testing.md)
- [præstation](performance.md)