# Performans

`fkv_bench` Sabit açar məkanında yükləri yazmaq, oxumaq və skan etmək üçün test edin.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Göstəricilərə ötürmə qabiliyyəti, hər mərhələdə gecikmə və pik RSS daxildir. iş axını buraxın x86_64 /
aarch64 üçün `fkv_bench` binar.