# fkv

fkv é um mecanismo de armazenamento KV integrado em Rust puro.

## característica

- Um thread de sistema operacional por fragmento, loop de trabalho cooperativo.
- COW B+tree, segmento IO somente anexado.
- formato de página/índice/manifesto/overflow/compressão.
- back-end de objeto local/memória/simulado/S3.
- modo de réplica autônomo/nuvem/standby.
- Sem C ABI, sem Python SDK; apenas Rust é a interface pública.

## Entrada

- [Arquitetura](architecture.md)
- [Crate](crates.md)
- [teste](testing.md)
- [desempenho](performance.md)