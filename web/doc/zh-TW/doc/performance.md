# 效能

`fkv_bench` 在固定 key 空間上測寫、讀、scan 負載。

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

指標含吞吐、各階段延遲分位與峰值 RSS。 release workflow 發布 x86_64 /
aarch64 的 `fkv_bench` 二進位。