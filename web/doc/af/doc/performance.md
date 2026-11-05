# Werkverrigting

`fkv_bench` Toetsskryf-, lees- en skandeerladings op 'n vaste sleutelspasie.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Aanwysers sluit in deurset, latency in elke stadium, en piek RSS. vrystelling werkvloei vrystelling x86_64 /
`fkv_bench` binêr vir aarch64.