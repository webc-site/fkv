# fkv

fkv é un motor de almacenamento KV integrado en Rust puro.

## característica

- Un fío do sistema operativo por fragmento, bucle de traballo cooperativo.
- COW B+árbore, segmento IO de só anexo.
- páxina/índice/manifiesto/desbordamento/formato de compresión.
- backend de obxectos local/memoria/simulado/S3.
- modo de réplica autónomo/nube/en espera.
- Sen C ABI, sen Python SDK; só Rust é a interface pública.

## Entrada

- [Arquitectura](architecture.md)
- [Crate](crates.md)
- [proba](testing.md)
- [rendemento](performance.md)