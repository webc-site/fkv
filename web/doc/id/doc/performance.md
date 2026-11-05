# Performa

`fkv_bench` Menguji beban tulis, baca, dan pindai pada ruang kunci tetap.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatornya mencakup throughput, latensi di setiap tahap, dan RSS puncak. rilis alur kerja rilis x86_64 /
`fkv_bench` biner untuk aarch64.