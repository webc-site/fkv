# Prestanda

`fkv_bench` Testa skriv-, läs- och skannaladdningar på ett fast nyckelutrymme.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatorer inkluderar genomströmning, latens i varje steg och topp RSS. release workflow release x86_64 /
`fkv_bench` binär för aarch64.