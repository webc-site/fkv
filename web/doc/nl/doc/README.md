# fkv

fkv is een pure Rust embedded KV-opslagengine.

## karakteristiek

- Eén OS-thread per scherf, coöperatieve werklus.
- KOE B+boom, segment-IO alleen toevoegen.
- pagina/index/manifest/overflow/compressieformaat.
- lokale/geheugen/gesimuleerde/S3-objectbackend.
- standalone/cloud/stand-by replicamodus.
- Geen C ABI, geen Python SDK; alleen Rust is de publieke interface.

## Ingang

- [Architectuur](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [prestatie](performance.md)