# fkv

fkv təmiz Rust quraşdırılmış KV saxlama mühərrikidir.

## xarakterik

- Hər parça üçün bir OS ipi, kooperativ iş dövrəsi.
- COW B+ağac, yalnız əlavə IO seqmenti.
- səhifə/indeks/manifest/daşmaq/sıxılma formatı.
- yerli/yaddaş/imitasiya edilmiş/S3 obyekt arxa sonu.
- müstəqil / bulud / gözləmə replika rejimi.
- C ABI yoxdur, Python SDK yoxdur; yalnız Rust ictimai interfeysdir.

## Giriş

- [Memarlıq](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [performans](performance.md)