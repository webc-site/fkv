# Prestazzjoni

`fkv_bench` Ittestja t-tagħbijiet tal-kitba, il-qari u l-iskennjar fuq spazju fiss taċ-ċavetta.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

L-indikaturi jinkludu throughput, latency f'kull stadju, u l-ogħla RSS. rilaxx tal-fluss tax-xogħol rilaxx x86_64 /
`fkv_bench` binarju għal aarch64.