# 性能

`fkv_bench` 在固定 key 空间上测写、读、scan 负载。

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

指标含吞吐、各阶段延迟分位与峰值 RSS。release workflow 发布 x86_64 /
aarch64 的 `fkv_bench` 二进制。
