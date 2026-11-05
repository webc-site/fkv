# ການທົດສອບ

## ທ້ອງຖິ່ນ

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` ໃຊ້ກໍລະນີທີ່ໃຊ້ຮ່ວມກັນກັບທະບຽນທົ່ວໂລກ; ເພີ່ມເມື່ອໃຊ້ `cargo test`ໂດຍກົງ
`--test-threads=1`.

## ກ່ອງບັນຈຸ Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

ສະຄຣິບໃຊ້ zigbuild ເພື່ອຂ້າມ compile binary ທົດສອບ Linux, ແລະຫຼັງຈາກນັ້ນເຮັດໃຫ້ມັນເຂົ້າໄປໃນຮູບພາບ Debian ຫນ້ອຍທີ່ສຸດທີ່ຈະດໍາເນີນການ.

## CI

`ci.yml`ແລ່ນ fmt, clippy, ການທົດສອບເຕັມ ແລະການທົດສອບການເຊື່ອມໂຍງ RustFS S3.
`linux-test.yml` ກັບຄືນຄືນໃນ Ubuntu 24.04 LTS ແລະຢືນຢັນ 4 KiB ຫນ້າ.