# fkv

fkv minangka mesin panyimpenan KV sing dipasang ing Rust murni.

## ciri khas

- Siji thread OS saben shard, loop kerja koperasi.
- COW B + wit, mung nambah segmen IO.
- format kaca / indeks / manifest / overflow / kompresi.
- lokal / memori / simulasi / S3 obyek backend.
- dewekan / maya / mode replika siyaga.
- Ora C ABI, ora Python SDK; mung Rust antarmuka umum.

## mlebu

- [Arsitektur](architecture.md)
- [Crate](crates.md)
- [tes](testing.md)
- [kinerja](performance.md)