# Performanță

`fkv_bench` Testați încărcările de scriere, citire și scanare pe un spațiu fix pentru chei.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indicatorii includ debitul, latența în fiecare etapă și RSS de vârf. lansează lansarea fluxului de lucru x86_64 /
`fkv_bench` binar pentru aarch64.