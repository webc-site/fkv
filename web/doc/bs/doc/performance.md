# Performanse

`fkv_bench` Testirajte učitavanje pisanja, čitanja i skeniranja na fiksnom ključnom prostoru.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatori uključuju propusnost, kašnjenje u svakoj fazi i vršni RSS. izdanje toka posla izdanje x86_64 /
`fkv_bench` binarni za aarch64.