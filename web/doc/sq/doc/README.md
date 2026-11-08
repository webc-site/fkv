# fkv

fkv është një motor ruajtës KV i ngulitur i pastër Rust.

## karakteristike

- Një fije OS për copë, cikli i punës bashkëpunues.
- LOPË B+pema, segmenti IO vetëm me shtojcë.
- faqe/indeks/manifest/format tejmbushje/kompresimi.
- backend i objektit lokal/memorie/simuluar/S3.
- modaliteti i kopjes së pavarur / resë / gatishmërisë.
- Nuk ka C ABI, nuk ka Python SDK; vetëm Rust është ndërfaqja publike.

## Hyrja

- [Arkitekturë](architecture.md)
- [Crate](crates.md)
- [provë](testing.md)
- [performancës](performance.md)