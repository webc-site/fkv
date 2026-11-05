# fkv

fkv je čisti Rust ugrađen KV motor za pohranu podataka.

## karakteristika

- Jedna OS nit po segmentu, kooperativna radna petlja.
- COW B+stablo, segment IO samo za dodavanje.
- format stranice/indeksa/manifesta/preljeva/kompresije.
- lokalna/memorija/simulirana/S3 pozadina objekta.
- samostalan / oblak / način rada replika u stanju pripravnosti.
- Nema C ABI, nema Python SDK; samo je Rust javno sučelje.

## Ulaz

- [Arhitektura](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [performanse](performance.md)