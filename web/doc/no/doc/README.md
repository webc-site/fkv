# fkv

fkv er en ren Rust-innebygd KV-lagringsmotor.

## karakteristisk

- Én OS-tråd per skjær, samarbeidende arbeidsløkke.
- COW B+tre, kun vedlegg segment IO.
- side/indeks/manifest/overflyt/komprimeringsformat.
- lokal/minne/simulert/S3-objekt-backend.
- frittstående / sky / standby replika-modus.
- Ingen C ABI, ingen Python SDK; bare Rust er det offentlige grensesnittet.

## Inngang

- [Arkitektur](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [ytelse](performance.md)