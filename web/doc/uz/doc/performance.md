# Unumdorlik

`fkv_bench` Ruxsat etilgan kalit maydonida yuklarni yozish, o'qish va skanerlashni sinab ko'ring.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Ko'rsatkichlar o'tkazish qobiliyati, har bir bosqichda kechikish va eng yuqori RSSni o'z ichiga oladi. ish oqimini chiqarish x86_64 /
`fkv_bench` aarch64 uchun ikkilik.