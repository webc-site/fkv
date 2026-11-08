# fkv

fkv, saf Rust'a gömülü bir KV depolama motorudur.

## karakteristik

- Parça başına bir işletim sistemi iş parçacığı, ortak çalışma döngüsü.
- COW B+ağacı, salt ekleme segmenti GÇ.
- sayfa/dizin/bildirim/taşma/sıkıştırma formatı.
- yerel/bellek/simüle edilmiş/S3 nesne arka ucu.
- bağımsız / bulut / bekleme çoğaltma modu.
- C ABI yok, Python SDK yok; yalnızca Rust genel arayüzdür.

## Giriş

- [Mimarlık](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [performans](performance.md)