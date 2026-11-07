# Výkon

`fkv_bench` Testujte zápis, čtení a skenování na pevném prostoru klíče.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikátory zahrnují propustnost, latenci v každé fázi a vrchol RSS. vydání pracovního postupu vydání x86_64 /
`fkv_bench` binární pro aarch64.