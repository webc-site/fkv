# fkv

fkv je čistě Rust embedded KV storage engine.

## charakteristický

- Jedno vlákno OS na fragment, kooperativní pracovní smyčka.
- COW B+strom, pouze připojení segmentu IO.
- formát stránky/indexu/manifestu/přetečení/komprese.
- místní/paměťový/simulovaný/objektový backend S3.
- samostatný / cloud / pohotovostní režim repliky.
- Žádné C ABI, žádné Python SDK; pouze Rust je veřejné rozhraní.

## Vjezd

- [Architektura](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [výkon](performance.md)