# fkv

fkv es un motor de almacenamiento KV integrado en Rust puro.

## característica

- Un subproceso del sistema operativo por fragmento, bucle de trabajo cooperativo.
- Árbol COW B+, segmento IO de solo agregar.
- formato de página/índice/manifiesto/desbordamiento/compresión.
- backend local/memoria/simulado/objeto S3.
- modo de réplica independiente/nube/en espera.
- Sin C ABI, sin SDK de Python; sólo Rust es la interfaz pública.

## Entrada

- [Arquitectura](architecture.md)
- [Crate](crates.md)
- [prueba](testing.md)
- [actuación](performance.md)