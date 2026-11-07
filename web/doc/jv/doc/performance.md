# Kinerja

`fkv_bench` Tes tulis, waca, lan pindai muatan ing papan tombol tetep.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikator kalebu throughput, latensi ing saben tahap, lan RSS puncak. rilis alur kerja rilis x86_64 /
`fkv_bench` binar kanggo aarch64.