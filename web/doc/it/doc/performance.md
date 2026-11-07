# Prestazioni

`fkv_bench` Prova i carichi di scrittura, lettura e scansione su uno spazio di chiave fisso.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Gli indicatori includono velocità effettiva, latenza in ogni fase e picco RSS. rilascio del flusso di lavoro rilascio x86_64 /
`fkv_bench` binario per aarch64.