# Rendiment

`fkv_bench` Proveu les càrregues d'escriptura, lectura i exploració en un espai de clau fixa.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Els indicadors inclouen el rendiment, la latència a cada etapa i l'RSS màxim. llançament del flux de treball x86_64 /
`fkv_bench` binari per a aarch64.