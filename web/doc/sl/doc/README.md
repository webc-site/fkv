# fkv

fkv je čisti mehanizem za shranjevanje KV, vgrajen v Rust.

## značilnost

- Ena nit OS na delček, kooperativna delovna zanka.
- COW B+drevo, segment IO samo za dodajanje.
- format strani/indeksa/manifesta/overflow/stiskanja.
- lokalno/pomnilniško/simulirano/objektno zaledje S3.
- samostojni/oblak/replikacijski način v pripravljenosti.
- Brez C ABI, brez Python SDK; samo Rust je javni vmesnik.

## Vhod

- [Arhitektura](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [uspešnost](performance.md)