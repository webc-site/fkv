# Performanse

`fkv_bench` Testirajte učitavanja pisanja, čitanja i skeniranja na fiksnom ključnom prostoru.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Pokazatelji uključuju propusnost, kašnjenje u svakoj fazi i vršni RSS. izdanje tijeka rada izdanje x86_64 /
`fkv_bench` binarni za aarch64.