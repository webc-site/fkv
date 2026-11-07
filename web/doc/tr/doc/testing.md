# test

## yerel

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Durum paylaşımlı genel kayıt defterini kullanın; `cargo test`'yi doğrudan kullanırken ekleyin
`--test-threads=1`.

## Linux kapsayıcıları

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Betik, Linux test ikili dosyasını çapraz derlemek için zigbuild'i kullanıyor ve ardından onu çalıştırılacak minimum Debian görüntüsüne yerleştiriyor.

## CI

`ci.yml` Fmt, Clippy, tam testleri ve RustFS S3 entegrasyon testlerini çalıştırın.