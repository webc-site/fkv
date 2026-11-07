# ทดสอบ

## ท้องถิ่น

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` ใช้กรณีการลงทะเบียนส่วนกลางที่ใช้ร่วมกัน เพิ่มเมื่อใช้ `cargo test` โดยตรง
`--test-threads=1`.

## คอนเทนเนอร์ลินุกซ์

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

สคริปต์ใช้ zigbuild เพื่อคอมไพล์ไบนารีทดสอบ Linux จากนั้นใส่ลงในอิมเมจ Debian ขั้นต่ำเพื่อรัน

## CI

`ci.yml` เรียกใช้การทดสอบ fmt, clippy, การทดสอบแบบเต็ม และการทดสอบการรวม RustFS S3