# fkv

fkv adalah mesin penyimpanan KV tertanam Rust murni.

## ciri

- Satu thread OS per shard, loop kerja kooperatif.
- COW B+tree, segmen IO khusus tambahan.
- halaman/indeks/manifest/overflow/format kompresi.
- backend objek lokal/memori/simulasi/S3.
- mode replika mandiri / cloud / siaga.
- Tanpa C ABI, tanpa Python SDK; hanya Rust yang merupakan antarmuka publik.

## Pintu masuk

- [Arsitektur](architecture.md)
- [Crate](crates.md)
- [tes](testing.md)
- [pertunjukan](performance.md)