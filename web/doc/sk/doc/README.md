# fkv

fkv je čistý KV ukladací modul s vstavaným Rustom.

## charakteristický

- Jedno vlákno operačného systému na zlomok, kooperatívna pracovná slučka.
- COW B+strom, IO segmentu iba na pridanie.
- formát stránky/indexu/manifestu/pretečenia/kompresie.
- lokálny/pamäťový/simulovaný/objektový backend S3.
- samostatný / cloud / pohotovostný režim repliky.
- Žiadne C ABI, žiadne Python SDK; iba Rust je verejné rozhranie.

## Vchod

- [Architektúra](architecture.md)
- [Crate](crates.md)
- [testovať](testing.md)
- [výkon](performance.md)