# fkv

fkv is 'n suiwer Rust ingebedde KV-bergingsenjin.

## kenmerk

- Een OS-draad per skerf, samewerkende werklus.
- KOE B+boom, byvoeg-slegs segment IO.
- bladsy/indeks/manifest/oorloop/kompressie-formaat.
- plaaslike / geheue / gesimuleerde / S3 voorwerp backend.
- selfstandige / wolk / bystand replika-modus.
- Geen C ABI, geen Python SDK; net Rust is die publieke koppelvlak.

## Ingang

- [Argitektuur](architecture.md)
- [Crate](crates.md)
- [toets](testing.md)
- [prestasie](performance.md)