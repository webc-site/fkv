# test

## locale

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Registro globale di condivisione dei casi d'uso; aggiungere quando si utilizza `cargo test` direttamente
<bid="1">.

## Contenitori Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Lo script utilizza zigbuild per eseguire la compilazione incrociata del binario di test di Linux, quindi lo inserisce nell'immagine Debian minima per l'esecuzione.

## CI

`ci.yml` Esegui fmt, clippy, test completi e test di integrazione RustFS S3.
`linux-test.yml` Restituito ogni notte in Ubuntu 24.04 LTS e affermato 4 pagine KiB.