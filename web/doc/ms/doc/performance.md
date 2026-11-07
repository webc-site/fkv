# Prestasi

`fkv_bench` Uji beban tulis, baca dan imbas pada ruang kunci tetap.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Petunjuk termasuk daya pemprosesan, kependaman pada setiap peringkat dan RSS puncak. keluarkan aliran kerja keluaran x86_64 /
`fkv_bench` binari untuk aarch64.