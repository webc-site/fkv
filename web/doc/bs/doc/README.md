# fkv

fkv je čist Rust ugrađen KV mehanizam za skladištenje.

## karakteristika

- Jedna OS nit po komadu, kooperativna radna petlja.
- COW B+stablo, segment IO samo za dodavanje.
- stranica/indeks/manifest/preljev/kompresija format.
- local/memory/simulated/S3 object backend.
- samostalni / cloud / standby replika mod.
- Nema C ABI, nema Python SDK; samo je Rust javni interfejs.

## Ulaz

- [Arhitektura](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [performanse](performance.md)