# fkv

fkv és un motor d'emmagatzematge KV integrat en Rust pur.

## característica

- Un fil del sistema operatiu per fragment, bucle de treball cooperatiu.
- COW B+tree, segment IO només per afegir.
- pàgina/índex/manifest/desbordament/format de compressió.
- backend d'objectes local/memoria/simulat/S3.
- Mode de rèplica autònom / núvol / en espera.
- Sense C ABI, sense SDK de Python; només Rust és la interfície pública.

## Entrada

- [Arquitectura](architecture.md)
- [Crate](crates.md)
- [prova](testing.md)
- [rendiment](performance.md)