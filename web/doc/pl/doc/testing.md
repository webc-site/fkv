# test

## lokalny

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Przypadek użycia współdzielonego rejestru globalnego; dodaj, używając bezpośrednio `cargo test`
`--test-threads=1`.

## Kontenery z Linuksem

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skrypt używa zigbuild do kompilacji krzyżowej testowego pliku binarnego Linuksa, a następnie umieszcza go w minimalnym obrazie Debiana w celu uruchomienia.

## CI

`ci.yml` Uruchom fmt, Clippy, pełne testy i testy integracji RustFS S3.