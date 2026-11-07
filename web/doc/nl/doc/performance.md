# Prestaties

`fkv_bench` Test schrijf-, lees- en scanbelastingen op een vaste sleutelruimte.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indicatoren zijn onder meer doorvoer, latentie in elke fase en piek-RSS. werkstroom vrijgave x86_64 /
`fkv_bench` binair voor aarch64.