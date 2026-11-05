# δοκιμή

## τοπικός

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Χρήση κοινόχρηστου καθολικού μητρώου περίπτωσης. προσθέστε όταν χρησιμοποιείτε το `cargo test` απευθείας
`--test-threads=1`.

## Κοντέινερ Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Το σενάριο χρησιμοποιεί το zigbuild για τη διασταυρούμενη μεταγλώττιση του δοκιμαστικού δυαδικού συστήματος Linux και στη συνέχεια το τοποθετεί στην ελάχιστη εικόνα του Debian για εκτέλεση.

## CI

`ci.yml` Εκτελέστε δοκιμές fmt, clippy, πλήρεις δοκιμές και δοκιμές ενοποίησης RustFS S3.
`linux-test.yml` Επιστρεφόταν κάθε βράδυ στο Ubuntu 24.04 LTS και διεκδίκησε σελίδες 4 KiB.