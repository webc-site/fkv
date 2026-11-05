# fkv

fkv è un motore di archiviazione KV integrato in Rust puro.

## caratteristica

- Un thread del sistema operativo per frammento, ciclo di lavoro cooperativo.
- COW B+albero, segmento IO di sola aggiunta.
- pagina/indice/manifest/overflow/formato di compressione.
- backend oggetto locale/memoria/simulato/S3.
- modalità di replica standalone/cloud/standby.
- Nessuna ABI C, nessun SDK Python; solo Rust è l'interfaccia pubblica.

## Entrata

- [Architettura](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [prestazione](performance.md)