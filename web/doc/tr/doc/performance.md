# Performans

`fkv_bench` Sabit bir anahtar alanında yazma, okuma ve tarama yüklerini test edin.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Göstergeler arasında aktarım hızı, her aşamadaki gecikme ve en yüksek RSS bulunur. iş akışı sürümünü yayın x86_64 /
`fkv_bench` aarch64 için ikili dosya.